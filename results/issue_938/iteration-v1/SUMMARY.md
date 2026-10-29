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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 111 mutants tested in 19m: 17 missed, 54 caught, 40 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_938/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.40s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #938’s HEAD, ranged/conditional GET, listings, bulk delete, and unsupported-operation probes: execution checks pass; the required architecture update is missing.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Criteria A–F define falsifiable gateway and fault outcomes, including retained 2xx status and mixed delete rows; prerequisite #853 is present in the supplied base (`brief.md:22`, `brief.md:50`, `reviewer-grounding.log:8`). |
| C2 Reproduction (red pre-fix) | N/A | This is the explicitly declared net-new API case: stashing production changes leaves the test unable to compile, so no behavioral pre-fix RED exists (`brief.md:98`, `reviewer-red.log:3`). |
| C3 Change | PASS | The code stays within the client/test scope and preserves the existing regression suites; the only added Wyrd dependency is for tests (`brief.md:86`, `target/crates/validate/Cargo.toml:65`, `reviewer-green.log:292`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared compile-only pre-fix posture and unconfirmed five-counterfactual evidence — all 67 targeted tests pass, but neither the stashed run nor the aggregate mutation log establishes the required behavioral reds (`brief.md:101`, `reviewer-red.log:3`, `reviewer-restored-green.log:20`, `gate-logs/C5-mutants.log:30`). |
| C5 Causal adequacy | PASS | The rejected attempt’s lost-2xx failure is directly exercised with stalled, trickling, cut, and unreadable bodies; head capture removes the information-loss cause, without an internal capability fallback (`target/crates/validate/src/s3/answer.rs:426`, `target/crates/validate/tests/s3_client_smoke_verbs.rs:1092`). |
| T1 Structure | PASS | The blackbox dependency boundary remains intact, and response capture, document interpretation, and operations have separate responsibilities (`target/crates/validate/src/s3/answer.rs:6`, `target/crates/validate/src/s3/document.rs:4`, `reviewer-ci.log:19`). |
| T2 Shape | PASS | Shared framing and strict XML readers keep old and new verbs subject to the same refusal rules; no parallel permissive parser was introduced (`target/crates/validate/src/s3.rs:415`, `target/crates/validate/src/s3/response.rs:463`). |
| T3 Runtime | PASS | Fault tests exercise finite body deadlines and bounded buffering; listing pagination has an explicit cap and the new deadline reads use one runtime clock (`target/crates/validate/src/s3/answer.rs:477`, `target/crates/validate/src/s3/document.rs:22`, `reviewer-restored-green.log:18`). |
| T4 Contribution | FAIL | The new public operations require a same-PR living architecture update, but the validator description still omits them and their response semantics (`target/AGENTS.md:154`, `target/crates/validate/src/s3/verbs.rs:127`, `target/docs/design/architecture/05-building-block-view.md:299`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm the recorded affected-path prior-art disposition — merged/closed/rejected work is named, but the supplied one-commit snapshot has no remotes or cited historical artifacts with which to mechanically settle it (`brief.md:124`, `reviewer-grounding.log:4`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether these client-level results are sufficient for #939’s smoke runner — real gateway and fault legs pass, while runner integration and operational fitness are intentionally outside this slice (`brief.md:89`, `brief.md:106`, `reviewer-green.log:331`). |

The confirmed patch defect is the missing living architecture update. Update the validator description at `target/docs/design/architecture/05-building-block-view.md:299` to cover the new operations, retained status beside body failures, listing continuation, and buffering limits. The source comments do not discharge the explicit same-PR requirement at `target/AGENTS.md:154`. This independently confirms the frozen batched-review finding; no functional patch defect was established.

Independent execution supports the implemented behavior:

- Ran `cargo test --offline --locked --manifest-path target/Cargo.toml -p wyrd-validate --test s3_client_smoke_verbs --test s3_client_roundtrip --test s3_client_upload_peers --test s3_client_nonconforming_responses`: 12 new tests and 55 unchanged regression tests passed (`reviewer-green.log:292`, `reviewer-green.log:313`, `reviewer-green.log:331`, `reviewer-green.log:349`). These exercise the actual gateway, recorded forbidden-name requests, and scripted malformed responses, rather than substituting code inspection for execution.
- Stashed the production change while keeping the new test, reproduced 21 compilation errors, restored the stash, and reran the new suite: 12 passed. The source and tracked diff match their initial content exactly (`reviewer-red.log:233`, `reviewer-restored-green.log:20`, `reviewer-grounding.log:11`). The five specific demonstrated regressions required at `brief.md:101` are not individually evidenced in the supplied gate logs; the aggregate mutant count cannot substitute for them.
- Ran `cargo xtask ci`: formatting, Clippy, builds, workspace tests, docs lint/render, spelling, conformance, repository scanners, and the simulation tier completed successfully. Its aggregate exit was 1 solely because two advisory scans could not lock the read-only home cache (`reviewer-ci.log:3808`, `reviewer-ci.log:3813`, `reviewer-ci.log:4445`). Both scans then passed against a workspace copy of the real advisory database; parsed policy was identical except for `advisories.db-path` (`reviewer-deny-local.log:7`, `reviewer-deny-all-local.log:1`). This is a resolved host restriction, not a patch defect. The brief’s `typos` and docs-renderer dependencies were exercised (`reviewer-ci.log:2`, `reviewer-ci.log:10`).
- Independently reran both TiKV/server feature Clippy commands; both exited 0 (`reviewer-tikv.log:202`), consistent with `gate-logs/host-tikv.log:111` and `gate-logs/host-tikv.log:209`.

Frozen evidence has three limits that should not be misreported as new defects:

- **C4-diff-cov:** coverage was not measured because the wrapper tried `origin/main`, whereas the brief explicitly requires the #853 integration base (`gate-logs/C4-diff-cov.log:10`, `brief.md:72`). The supplied target contains that prerequisite and builds successfully. This is a gate-base caveat, not an application or compilation failure of the patch.
- **C5-mutants:** the log reports 111 tested, 54 caught, 17 missed, and 40 unviable (`gate-logs/C5-mutants.log:30`). I inspected the listed survivors, including header enumeration, V1 marker filtering, decimal-size validation, and probe framing classification. They expose residual test coverage limits; the survivor count alone does not establish incorrect production behavior. Mutation testing is non-gating under `target/AGENTS.md:72`. The instance-scoped mutation/review wrappers are absent here; their available frozen logs were adjudicated rather than treating wrapper absence as a finding.
- **T4-contribution: N/A.** The contribution artifacts are deliberately drafted after Check; their substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). This deferred row creates no human-clearance item.

The prior-art record does explicitly search by `crates/validate/src/s3.rs` and `crates/validate/src/s3/`, names merged #859/#865, open #934, no closed-unmerged PRs, and the rejected #743 attempt (`brief.md:124`). Its historical evidence is not present in this isolated snapshot; T5 asks for confirmation of that disposition, not a repeat code change.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #938 (743.2, S3 client smoke verbs)

I rebuilt the patched tree in a scratch copy (cargo 1.96.0 was available). `s3_client_smoke_verbs` passes 12/12. `s3_client_roundtrip`, `s3_client_upload_peers` and `s3_client_nonconforming_responses` pass without changes, so (E) holds. Then I broke the production code by hand to recreate the brief's required reds. Red 1 (head lost on a body failure), red 2 (cut listing accepted), red 4 (listing stops after page 1) and red 5 (`<Error>` rows dropped) each make the new test fail. Red 3 I did not recreate, but (B) compares the request path byte for byte, so a client that rewrote the name would fail it. The core claim, that a 2xx head survives a stalled, trickled, cut or unreadable body, survived every attack I tried. What breaks are the edges below.

- NEEDS-HUMAN [impl] — **A correct 2xx success document is never tested, so the "unexpected-support" path can break without the suite noticing.** `crates/validate/src/s3/answer.rs:321-323` (`SuccessBody::Document`) and the root names at `crates/validate/src/s3/verbs.rs:104-114` are never exercised by a 2xx carrying the right document. The only `Outcome::Success` assertion in the test is HEAD's empty body (`crates/validate/tests/s3_client_smoke_verbs.rs:289`). I ran two breakages and both left the suite at 12/12 green: (a) the `Document` arm refuses every whole document; (b) `"CopyObjectResult"` mistyped as `"CopyObjectResul"`. Under either one, a server that really implements CopyObject (or CreateMultipartUpload, ListBuckets, and so on) and answers `200` plus a whole, correct document is reported as `Outcome::Unreadable`. That downgrades a real success, against the invariant at `answer.rs:2-4`. cargo-mutants does not mutate string literals, so C5 cannot see this. Fix: add a scripted-endpoint leg with one `200` + whole success document per `Document` root, each asserting `Outcome::Success`.

- NEEDS-HUMAN [impl] — **Some bucket names S3 forbids are refused or rerouted by the SDK, despite the doc's claim.** `crates/validate/src/s3/verbs.rs:35-38` says the bucket probes "send whatever name they are given ... a name those rules forbid reaches the endpoint as given". I tested this against a scripted endpoint:
  - `Probe::DeleteBucket` and `Probe::CreateBucket` with `wyrd-probe--x-s3`: the SDK's S3 Express handling (the directory-bucket path the SDK takes for names ending in `--x-s3`) sent **`GET /wyrd-probe--x-s3/?session`**, a CreateSession request, instead of the probe. It then reported the endpoint's `400` as `S3Error::NoResponse`. So a different operation went out, and a server that did answer was reported as "no answer".
  - `arn:aws:s3:::wyrd-probe`: refused before sending ("Path-style addressing cannot be used with ARN buckets"), but classified `NoResponse`. The `s3.rs` docs say `NoResponse` counts against the server as `availability`.

  The names the test uses (`s3_client_smoke_verbs.rs:82-91`) all pass, and so does 743.3's chosen `wyrd-validate-probe-`, so the safety rule is not broken today. But the doc and the test name (`bucket_probes_send_a_name_s3_forbids_exactly_as_given`, `:753`) claim more than holds. Fix: narrow the claim and name the names the SDK reroutes or refuses, or turn off S3 Express session auth on this client. Either way, report a refused name as `RequestNotBuilt`, not `NoResponse`.

- NEEDS-HUMAN [human] — **S3's documented "200 with an `<Error>` body" is reported as unreadable, and its S3 code is dropped.** S3 documents that CopyObject and CompleteMultipartUpload (both probes here) can send a `200` head and then an `<Error>` document. In `answer.rs:336-337` a 2xx only goes through the success reader. I scripted `200` + `<Error><Code>InternalError</Code>…` for `Probe::CopyObject`. Result: `Outcome::Unreadable { detail: "…root is <Error>, not <CopyObjectResult>" }`. The status and raw body are kept, but the S3 code and message are not reported as typed fields. The brief asks for "the S3 error code" in each report. `answer.rs:139-143` makes this a deliberate design choice, and how 743.1 judges a `200`+`<Error>` (unsupported, error, or malformed) depends on it. So this needs a decision between 743.1 and 743.2, not just a rebuild.

- NEEDS-HUMAN [impl] — **The T4 docs-currency blocker is real.** The living architecture doc describes this client's surface and budget at `docs/design/architecture/05-building-block-view.md:299`. It says the buffered budget applies to "every error response, and every PUT or DELETE response", and it counts requests as "PUT, GET and DELETE requests together". After this patch both are stale: listing pages, delete results, probes, and non-2xx ranged or conditional GETs are now read under the 64 KiB budget by the client itself. The doc also does not mention the new verbs, the 100-page listing cap, or `Answer`/`Outcome`. The patch touches no file under `docs/`. The rubric calls the docs-currency update a merge requirement, so `T4-batch-review`'s gating fail is warranted, not noise.

- NEEDS-HUMAN [impl] — **Four rules the brief or the rubric care about have no test. The code is correct today; nothing stops a regression.**
  - `document.rs:251-256` (`decimal`): the surviving `||`→`&&` mutant means `<Size>+7</Size>` would be read as 7 with no test failing. Rust's `u64::from_str` accepts a leading `+`, which is exactly the rubric's "no `+`/`-` signs via `from_str`" class.
  - `document.rs:180-183`: the surviving `delete !` mutant means a page's `<NextMarker>` can be ignored. The gateway always sends NextMarker equal to the last key (`crates/gateway-s3/src/lib.rs:681-691`), so the two can never differ in (A).
  - `ListingError::TooManyPages` (`document.rs:22`, `verbs.rs:203-224`) is never reached. This cap is the only thing that bounds a listing call against an endpoint that answers every page with another token.
  - `Head::header_lines` and `Head::fields` (`answer.rs:99-111`) are public API for 743.1 to read repeated headers, and both survive replacement with an empty iterator.

  Fix: one small scripted case each.

- **C4-diff-cov's "fail" is not a refutation.** The patch is built on the integration line that carries #853, not on `origin/main` (see the brief's ordering note), so `patch.diff` cannot apply on main. Diff coverage was simply not measured. C5 mutants (54 caught) and my four reds partly make up for it. C4-verify's UNVERIFIABLE was declared in advance and is expected for net-new API.

**Attempted to refute and could not:**
- 2xx head lost to the SDK. The capture records the head at `read_after_transmit`, before the SDK's `S3ExpiresInterceptor` rewrites headers. It swaps the body out before the SDK reads it, so SDK deserialization errors after that are ignored (`verbs.rs:391-393`).
- Unbounded waits. `send()` is bounded by the client-wide operation timeout (`s3.rs` `with_deadlines`), and `read_whole` by idle, whole-body and budget limits.
- (D) refusals for cut, junk, second-document, overrun and over-budget bodies, with and without chunked framing, on page 1 and on a later page.
- (F) row order and exact key, code and message.
- HEAD 404 reported as `NoBody`, not the SDK's made-up `NotFound`.
- Rule breaches: no `wyrd-*` crate in `[dependencies]`, and `lib.rs` is untouched.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/verbs.rs:127`: The new public client operations land without the same-PR living architecture update required by the standing docs-currency rule. `docs/design/architecture/05-building-block-view.md:299` still describes the earlier client; update it for the added verbs, preserved response heads, pagination limit, and bulk-delete results. This corroborates the existing T4 batch-review finding.

No introduced runtime correctness bugs or substantive reuse, simplification, or efficiency findings identified. Frozen CI evidence records all 12 new smoke-verbs tests and the three existing client suites passing; no tests were rerun against the read-only target.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared compile-only pre-fix posture and unconfirmed five-counterfactual evidence — all 67 targeted tests pass, but neither the stashed run nor the aggregate mutation log establishes the required behavioral reds (`brief.md:101`, `reviewer-red.log:3`, `reviewer-restored-green.log:20`, `gate-logs/C5-mutants.log:30`).
- [ ] T5 Judgment — Confirm the recorded affected-path prior-art disposition — merged/closed/rejected work is named, but the supplied one-commit snapshot has no remotes or cited historical artifacts with which to mechanically settle it (`brief.md:124`, `reviewer-grounding.log:4`).
- [ ] Validation — fitness-to-purpose — Decide whether these client-level results are sufficient for #939’s smoke runner — real gateway and fault legs pass, while runner integration and operational fitness are intentionally outside this slice (`brief.md:89`, `brief.md:106`, `reviewer-green.log:331`).
- [ ] **A correct 2xx success document is never tested, so the "unexpected-support" path can break without the suite noticing.** `crates/validate/src/s3/answer.rs:321-323` (`SuccessBody::Document`) and the root names at `crates/validate/src/s3/verbs.rs:104-114` are never exercised by a 2xx carrying the right document. The only `Outcome::Success` assertion in the test is HEAD's empty body (`crates/validate/tests/s3_client_smoke_verbs.rs:289`). I ran two breakages and both left the suite at 12/12 green: (a) the `Document` arm refuses every whole document; (b) `"CopyObjectResult"` mistyped as `"CopyObjectResul"`. Under either one, a server that really implements CopyObject (or CreateMultipartUpload, ListBuckets, and so on) and answers `200` plus a whole, correct document is reported as `Outcome::Unreadable`. That downgrades a real success, against the invariant at `answer.rs:2-4`. cargo-mutants does not mutate string literals, so C5 cannot see this. Fix: add a scripted-endpoint leg with one `200` + whole success document per `Document` root, each asserting `Outcome::Success`.
- [ ] **Some bucket names S3 forbids are refused or rerouted by the SDK, despite the doc's claim.** `crates/validate/src/s3/verbs.rs:35-38` says the bucket probes "send whatever name they are given ... a name those rules forbid reaches the endpoint as given". I tested this against a scripted endpoint:
- [ ] **S3's documented "200 with an `<Error>` body" is reported as unreadable, and its S3 code is dropped.** S3 documents that CopyObject and CompleteMultipartUpload (both probes here) can send a `200` head and then an `<Error>` document. In `answer.rs:336-337` a 2xx only goes through the success reader. I scripted `200` + `<Error><Code>InternalError</Code>…` for `Probe::CopyObject`. Result: `Outcome::Unreadable { detail: "…root is <Error>, not <CopyObjectResult>" }`. The status and raw body are kept, but the S3 code and message are not reported as typed fields. The brief asks for "the S3 error code" in each report. `answer.rs:139-143` makes this a deliberate design choice, and how 743.1 judges a `200`+`<Error>` (unsupported, error, or malformed) depends on it. So this needs a decision between 743.1 and 743.2, not just a rebuild.
- [ ] **The T4 docs-currency blocker is real.** The living architecture doc describes this client's surface and budget at `docs/design/architecture/05-building-block-view.md:299`. It says the buffered budget applies to "every error response, and every PUT or DELETE response", and it counts requests as "PUT, GET and DELETE requests together". After this patch both are stale: listing pages, delete results, probes, and non-2xx ranged or conditional GETs are now read under the 64 KiB budget by the client itself. The doc also does not mention the new verbs, the 100-page listing cap, or `Answer`/`Outcome`. The patch touches no file under `docs/`. The rubric calls the docs-currency update a merge requirement, so `T4-batch-review`'s gating fail is warranted, not noise.
- [ ] **Four rules the brief or the rubric care about have no test. The code is correct today; nothing stops a regression.**
- [ ] `crates/validate/src/s3/verbs.rs:127`: The new public client operations land without the same-PR living architecture update required by the standing docs-currency rule. `docs/design/architecture/05-building-block-view.md:299` still describes the earlier client; update it for the added verbs, preserved response heads, pagination limit, and bulk-delete results. This corroborates the existing T4 batch-review finding.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_938/review-b
- [ ] **The bulk-delete criterion can pass without exercising error rows.** `brief.md:27-28` requires `<Deleted>` and `<Error>` rows “exactly as the document carried them” followed by “the keys are then gone,” but specifies no mixed-result fixture. On the target, `crates/gateway-s3/src/lib.rs:2237-2240,2260-2272` distinguishes successful deletes from per-key failures while still returning HTTP 200; the existing happy-path test explicitly expects zero errors (`crates/server/tests/s3_delete_objects.rs:448-452`). A client that always returns an empty error list could satisfy the proposed happy-path assertions. Require a scripted mixed `<DeleteResult>` with exact key/code/message assertions and a demonstrated red when an error row is dropped; restrict the disappearance assertion to the successful real-gateway case.
- [ ] **The prerequisite exists in batch metadata but is absent from the supplied source base.** `brief.md:55-59` targets `getwyrd/wyrd @ main` and asserts #853 is “on the integration line”; `dependency-state.json:2-5` confirms an existing COMPLETE bundle. The target remote resolves to that repository, and HEAD/main/origin/main resolve to `4bac53878edfecfea34ad849edbe010f2d79d1db`, but target `crates/validate/src/s3.rs:264-268,313-317` still explicitly defers response hardening to #853. The cited `s3/response.rs`, `s3/wire.rs`, and `s3_client_nonconforming_responses.rs` are absent. Also, `roxmltree` is only a dev-dependency (`crates/validate/Cargo.toml:42-43`), contradicting `brief.md:68-69`'s production-dependency assumption. Reconcile the target/stack instructions with an actual #853-containing base and refresh citations there. Otherwise Do must recreate prerequisite work outside the declared scope, and criterion (E)'s “pass unmodified” names a nonexistent test.
- [ ] **The claimed tracker history cannot be checked from the supplied inputs.** Neither `notes.json` nor `sources/` exists in this sandbox. In particular, `brief.md:99-103` says the rejected attempt “had one finding,” and `brief.md:33-35` attributes forbidden-bucket requests to 743.3's safety rule, without supplying either underlying record. This is an evidence gap, not proof those claims are false. Include the relevant tracker/review excerpts and source identifiers in the revised brief so the scope split, prior failure, and safety constraint can be independently checked.
- [ ] size backstop — this slice is behaving oversized: patch is 128 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Core behaviour held under adversarial review (2xx head survives stalled/cut/unreadable bodies, strict listing/DeleteResult refusals, mixed DeleteResult rows); keep that design. Close these gaps: - Docs currency (gating T4): update docs/design/architecture/05-building-block-view.md:299 for the new verbs, retained 2xx heads beside body failures, listing continuation and the 100-page cap, bulk-delete rows, and the widened 64 KiB buffered budget (listings, delete results, probes, non-2xx ranged/conditional GETs). This doc edit is in scope for this slice. - Add scripted-endpoint legs: 200 + whole correct success document for each Document root (verbs.rs:104-114) -> Outcome::Success; <Size>+7</Size> refused (document.rs:251-256); a NextMarker that differs from the last key is followed (document.rs:180-183); ListingError::TooManyPages reached; Head::header_lines / Head::fields return repeated headers (answer.rs:99-111). - Bucket names: the "sent exactly as given" claim (verbs.rs:35-38, test name at s3_client_smoke_verbs.rs:753) overstates it — the SDK reroutes `--x-s3` names to a CreateSession request and refuses ARN names before sending. Either disable S3 Express session auth on this client or narrow the claim and name those cases; in both cases report a name refused before sending as RequestNotBuilt, not NoResponse. - A 2xx carrying an <Error> document (S3 does this for CopyObject / CompleteMultipartUpload): keep the 2xx status, and also expose the S3 code and message as typed fields instead of only Outcome::Unreadable (answer.rs:336-337), matching the brief's "report the S3 error code".
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
