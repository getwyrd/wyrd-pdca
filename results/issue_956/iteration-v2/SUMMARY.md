# Result — issue 956 / validate-s3-client-deletes-and-bucket-probes

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: with #955 in, the client still cannot send bulk `DeleteObjects` or one request
  for each operation the matrix may declare unsupported. And a probe does not reach the
  endpoint it was given with the bucket name it was given: the SDK's endpoint rules reroute
  or refuse some names (S3 Express `--x-s3` / `--xa-s3`, Outposts-alias `--op-s3`, `arn:`)
  before anything is sent — exactly what 743.3's bucket-safety rule cannot tolerate (a
  `DeleteBucket` probe must name a bucket S3's rules forbid, so it can never delete one it
  does not own). A mixed `<DeleteResult>` has no evidence of a failed key but its `<Error>`
  rows (the gateway answers `200` either way, `crates/gateway-s3/src/lib.rs:2293-2296`).
- Success criterion: the tests this child adds to `crates/validate/tests/s3_client_smoke_verbs.rs`
  pass, against the in-process gateway and, for the fault and routing legs, a scripted loopback
  endpoint or relay:
  - **(A) Bulk delete against the real gateway.** DeleteObjects of N existing keys → every key
    under `<Deleted>`, no `<Error>` row; the keys are then gone (HEAD 404). Error rows are (F)'s.
  - **(B) Every unsupported operation reaches the gateway** and is reported with status and S3
    code: `CopyObject`, the six multipart operations (`Probe` names them) and
    `GetBucketLocation` 501 `NotImplemented`; `CreateBucket`, `DeleteBucket`, `ListBuckets`
    400 `InvalidRequest`; `HeadBucket` 400 with no body. **Each probe is also checked as the
    request it sent** (added 2026-10-23: the gateway answers every recognised bucket
    subresource from one `501` branch and every object subresource from another,
    `crates/gateway-s3/src/lib.rs:1797-1802`, `:1836-1847`, so the answer alone cannot tell
    which operation was sent). Through the relay, exactly one request per probe, whose method,
    path, operation parameters and distinguishing headers are those of its operation:
    `CopyObject` `PUT /<b>/<key>` with `x-amz-copy-source: <b>/<source>`;
    `CreateMultipartUpload` `POST /<b>/<key>?uploads`; `UploadPart` `PUT
    /<b>/<key>?partNumber=1&uploadId=<id>`; `CompleteMultipartUpload` `POST
    /<b>/<key>?uploadId=<id>`; `AbortMultipartUpload` `DELETE /<b>/<key>?uploadId=<id>`; `ListParts`
    `GET /<b>/<key>?uploadId=<id>`; `ListMultipartUploads` `GET /<b>/?uploads`;
    `GetBucketLocation` `GET /<b>/?location`; `CreateBucket` `PUT /<name>/`, `DeleteBucket`
    `DELETE /<name>/`, `HeadBucket` `HEAD /<b>/`; `ListBuckets` `GET /`. Of the operation
    parameters `uploads`, `uploadId`, `partNumber`, `location` and `delete`, each request
    carries exactly its own, with the values given (the `uploadId` as the probe named it),
    and none of another's; other query parameters the SDK adds are ignored, and so is the
    body framing (the SDK may frame a body with checksums, so no length is asserted). Method,
    path, that parameter set and `CopyObject`'s `x-amz-copy-source` together tell the twelve
    probes apart from each other and from the supported verbs. The salvage's
    `ProbeCase` checks one parameter per probe by presence only
    (`probe_reaches_the_gateway`, iteration-v3 patch lines 2761-2815); extend it here.
    **Bucket names reach the endpoint as given**, on an IP
    endpoint (`http://127.0.0.1:{port}`) AND a host-name endpoint (`http://localhost:{port}`):
    `CreateBucket` / `DeleteBucket` naming a bucket S3's rules forbid (one ending in `-`), a
    host-safe and a non-host-safe `--x-s3` name, an `--xa-s3` name, an Outposts-alias-shaped
    `--op-s3` name and an `arn:` name each arrive as ONE request, `PUT|DELETE /<name>/`, the
    name one percent-encoded path segment after the endpoint's own path, `Host:` the
    configured endpoint, signed for `s3` in the configured region. `ListBuckets` addresses the
    endpoint itself. A name that cannot be sent at all is `RequestNotBuilt` (nothing sent),
    never `NoResponse`. A request that reached the connector and got no answer is classified
    as the existing verbs classify it (`S3Client::classify`, `crates/validate/src/s3.rs:339-356`
    on `main`; revised 2026-10-23): the connect deadline expiring is `Timeout { phase:
    Connect }`, the operation deadline expiring is `Timeout { phase: Operation }`, and any
    other loss before a response head (refused, reset, closed unanswered) is `NoResponse`;
    none of them is ever `RequestNotBuilt`. A probe is shown under both deadlines, as
    `connect_deadline_expires_as_a_connect_timeout` and
    `operation_deadline_expires_as_an_operation_timeout` show it for `get_object`
    (`crates/validate/tests/s3_client_roundtrip.rs:1274`, `:1296`; the saturated listener
    `:1258`). The two unsendable names (empty or all white space; a URL past an `http`
    URI's 65,534 bytes) are named in the code's docs and tested.
  - **(C) A 2xx head survives its body, on every probe that can carry one.** A scripted
    endpoint answers `200` and then (i) stalls the body past the body-idle deadline, (ii)
    trickles it past the whole-body deadline, (iii) cuts the connection mid-body, (iv) sends a
    body that is not the operation's success document. Each report carries status 200 and the
    body failure beside it. A `200` whose body is an S3 `<Error>` document (S3 does this for
    `CopyObject` / `CompleteMultipartUpload`) keeps the 200 AND reports that document's
    `<Code>` and `<Message>` as typed fields.
  - **(D) An unreadable `<DeleteResult>` is refused** by the rules #955 applies to a listing
    page (cut short, followed by junk or a second document, past `Content-Length`, past
    `BUFFERED_BODY_BUDGET`): a typed refusal that keeps the 200, never a short delete result.
    A document of exactly the budget is read.
  - **(E)** The three existing `s3_client_*` test files pass unmodified; #955's tests still
    pass.
  - **(F) A mixed `<DeleteResult>` is reported row by row.** A scripted endpoint answers
    `POST ?delete` with `200`, two `<Deleted>` rows and one `<Error>` row (`<Key>`,
    `<Code>AccessDenied</Code>`, `<Message>`). The result lists exactly those two deleted keys
    and exactly one error row with that key, code and message as sent, in document order.
  - **(G) The success paths are pinned.** Against the scripted endpoint: a `200` with each
    probe's whole, correct success document (or no body where its success has none) is a
    success; a 304 that frames a chunked body is refused for a probe that is not a GET or HEAD.
    Each success leg first checks the request head the endpoint recorded against that probe's
    form in (B) (revised 2026-10-23), so a success answered to the wrong request cannot pass.
  - **(H) The living architecture doc matches the code.** The `validate` paragraph,
    `docs/design/architecture/05-building-block-view.md:299` on `main` (`:301` on the
    integration line), describes the new verbs — #955's and this child's — the 2xx head
    kept beside a body failure, listing continuation and its page cap, bulk-delete rows, the
    buffered budget's reach, and the endpoint resolution with its named exceptions and the
    two region changes. No sentence in it or in the code's docs claims more than the tests
    show. The salvage's new paragraph is the text to carry over.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: bulk delete, the probes and their typed results in `crates/validate/src/s3/*`;
  the endpoint resolution in `crates/validate/src/s3.rs`; the tests added to the test file;
  the `validate` paragraph of `docs/design/architecture/05-building-block-view.md`. / out of
  scope: #955's core and read verbs beyond extending their docs; judging answers (743.1);
  the runner, ledger, version source and arguments (743.3); `lib.rs`; the multipart success
  path (#508, #750); any change to the existing verbs beyond what the shared resolution
  implies; any `wyrd-*` crate in `[dependencies]`; any new production dependency;
  `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 97.7% — 258 of 264 instrumentable changed lines executed (floor 80%); 264 of 591 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 51 mutants tested in 10m: 34 caught, 14 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_956/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.61s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No patch defect found in #956's bulk DeleteObjects, unsupported-operation probes, and endpoint-preserving routing; fitness-to-purpose remains a human decision.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The contract is falsifiable at both request and response boundaries, and the prior iteration's five corrections are explicit acceptance criteria (`brief.md:42`, `brief.md:237`). |
| C2 Reproduction (red pre-fix) | N/A | These APIs are new; stashing their production changes independently produces the declared missing-API compile failure, not a behavioral red (`brief.md:179`, `reviewer-test-red.log:3`). |
| C3 Change | PASS | The client reports missing/unrequested delete keys without assuming the caller's verdict, and explicitly leaves bucket ownership safety to the caller, matching the revised scope (`crates/validate/src/s3/verbs.rs:32`, `crates/validate/src/s3/verbs.rs:294`, `docs/design/architecture/05-building-block-view.md:303`). |
| C4 Verification (red→green) | PASS | Independently ran 186 passing validator tests; the stashed production tree fails compilation and the restored smoke suite passes all 47 tests; frozen CI and 97.7% instrumentable diff coverage support the wider checks (`reviewer-test-green.log:269`, `reviewer-test-final.log:54`, `gate-logs/C4-ci.log:4656`, `gate-logs/C4-diff-cov.log:322`). |
| C5 Causal adequacy | PASS | Replacing SDK endpoint rules removes the routing cause; no capability fallback masks it, and focused reruns catch all three mutants that timed out in the frozen campaign (`crates/validate/src/s3.rs:186`, `reviewer-mutants-confirm.log:2`, `reviewer-mutants-url.log:2`). |
| T1 Structure | PASS | The blackbox dependency boundary and shared bounded response-reading path remain intact, with no new production dependency (`crates/validate/Cargo.toml:29`, `crates/validate/src/s3/verbs.rs:455`, `gate-logs/C4-ci.log:28`). |
| T2 Shape | PASS | The twelve operations remain distinguishable by their recorded requests, and malformed delete rows cannot silently disappear from a successful result (`crates/validate/tests/s3_client_smoke_verbs.rs:995`, `crates/validate/src/s3/document.rs:237`). |
| T3 Runtime | PASS | Real gateway deletion, both probe deadlines, and body failures after a 200 are exercised successfully, including preservation of the response status (`crates/validate/tests/s3_client_smoke_verbs.rs:551`, `crates/validate/tests/s3_client_smoke_verbs.rs:1428`, `crates/validate/tests/s3_client_smoke_verbs.rs:1959`, `reviewer-test-final.log:54`). |
| T4 Contribution | N/A | Contribution prose is intentionally drafted after Check; the substantive audit must run at publish, so this deferred row requires no human clearance (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The carry-forward corrections are covered, and prior-art checks by affected filename found the merged predecessors and no overlap among all 19 returned closed-unmerged PRs (`brief.md:237`, `reviewer-prior-art.json:8`, `reviewer-prior-art-closed.json:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this tested client contract is sufficient for #939's smoke scenario — callers still choose a safe bucket name and judge per-key discrepancies, which determine whether the validator can safely produce meaningful verdicts (`crates/validate/src/s3/verbs.rs:32`, `crates/validate/src/s3/verbs.rs:294`, `brief.md:189`). |

Source citations are grounded in `$PDCA_TARGET` (`./target`); evidence citations are relative to this review directory. The target contains the prerequisite implementation, and its final git diff is byte-identical to `patch.diff`. No stale-target caveat applies.

The independent checks support the implementation, with the following limits:

- **Reproduction and tests:** `cargo test -p wyrd-validate --offline` passed all 186 tests. Stashing only the four changed production Rust files while retaining the new tests made `s3_client_smoke_verbs` fail with missing `Probe`, `DeleteRow`, `KeyError`, `probe`, and `delete_objects` APIs. After stash restoration, all 47 smoke tests passed, and they passed again on the repository-pinned toolchain after mutation testing. This is the brief's declared compile-red posture; the frozen wrapper itself proves only green and incorrectly says no separate test file exists (`gate-logs/C4-verify.log:60`).
- **Mutation evidence:** the frozen campaign reports 34 caught, 14 unviable, three timed out, and no surviving mutant (`gate-logs/C5-mutants.log:10`). Restricting the rerun to `bucket_probes_send_every_name` caught both scheme mutations. The constant-URL mutation initially failed compilation because of denied unused-code warnings; rerunning it with `--cap-lints=true` caught it by a test assertion. The three original timeouts therefore do not establish a patch defect. An earlier broad reviewer filter also included an unchanged interceptor-name mutation; that diagnostic-name survivor is outside this patch and is not a finding.
- **Other gates and dependencies:** independently ran `typos`, docs lint, docs rendering with link audit (101 pages), formatting, and diff checks successfully. Both external dependencies named in `brief.md:162` were exercised. The instance-scoped full CI, coverage, batch-review, contribution, and TiKV rows were adjudicated from their frozen logs: CI completed, coverage scored 258/264 instrumentable lines with 327 unscored lines, batch review reported zero blocking findings, contribution was deferred, and both TiKV compiler commands finished (`gate-logs/C4-ci.log:11`, `gate-logs/C4-diff-cov.log:322`, `gate-logs/T4-batch-review.log:10`, `gate-logs/host-tikv.log:211`). These wider runs are captured gate evidence, not claimed as independent reruns.
- **Rubric and prior art:** applied `AGENTS.md:137`, including strict protocol parsing, explicit missing-entry reporting, bounded work, dependency direction, and architecture-doc currency. Checked merged PR metadata by affected path/name and verified actual changed filenames, including [#934](https://github.com/getwyrd/wyrd/pull/934), [#865](https://github.com/getwyrd/wyrd/pull/865), [#859](https://github.com/getwyrd/wyrd/pull/859), and the read-client prerequisite [#1034](https://github.com/getwyrd/wyrd/pull/1034). Separately enumerated closed-unmerged PRs and checked all 19 file lists: none touches any of the six affected files. The local rejected attempts are recorded in `brief.md:207` and `brief.md:237`; their requested corrections are present and tested. Settled deferrals were not reopened.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 956

**Verdict: the routing, probe and row-reading code held up, and I could not break the claimed reds. I did refute one sentence of the docs with a panic, and two other claims in the docs and tests.** Each finding below was run on a scratch copy of `$PDCA_TARGET`, unless it says "by reading".

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/src/s3.rs:52-54` says a URL longer than an `http` URI allows is `RequestNotBuilt`, but `S3Client::probe` (`crates/validate/src/s3/verbs.rs:343`) **panics** when the key or upload id makes the URL too long. Failing case: `client.probe(&Probe::CreateMultipartUpload { key: "k".repeat(70_000) })` unwinds at `aws-sdk-s3-1.155.0/src/operation/create_multipart_upload.rs:370` (`expect("valid request")`, `InvalidUri(TooLong)`) instead of returning `Err`. The same panic hit `CopyObject`, `UploadPart`, `CompleteMultipartUpload` and `ListParts` with a long key, and `UploadPart` and `AbortMultipartUpload` with a long upload id. Sweeping `ListParts` key length: 65,500 bytes is `RequestNotBuilt`, 65,510 and up panic. The tests only try a long bucket name (`a_request_stopped_before_it_reaches_the_connector…`), which goes through the endpoint and is reported correctly. The inputs come from the caller, and `verbs.rs:34` says the caller chooses the upload id, so no server can trigger this and S3's own 1,024-byte key limit makes it unlikely in a real run. Fix: refuse an overlong key or upload id before calling the SDK, as `RequestNotBuilt`, with a test. `head_object` panics the same way, but that comes from #955, so it is not counted here.

- NEEDS-HUMAN [impl] — No test shows that `delete_objects` keeps its 200 when the body fails on the wire. `docs/design/architecture/05-building-block-view.md:303` says a body that "stalls, is cut, overruns its framing or runs past the budget after a 2xx head is reported beside that 2xx", and that paragraph covers bulk delete, but only stalled, trickled and cut bodies on listings and probes are tested (`crates/validate/tests/s3_client_smoke_verbs.rs:1881`, `:1964`). I changed `verbs.rs` so a body timeout in `delete_objects` returns `NoResult::NoAnswer(Timeout)`, then separately so a cut body returns `NoResponse`. Both changes drop the 200, and all 47 tests still pass. Changing its `Capture::new(false)` to `true` (`verbs.rs:317`) also passes all 47, so a chunked `304` to this POST is untested too. The production code is correct today, through `settle`; the gap is in the tests, and brief (H) says no sentence may claim more than the tests show. Fix: run `BODY_FAULTS` and the `304` case through `delete_objects`.

- NEEDS-HUMAN [impl] — A processing instruction inside `<DeleteResult>` is skipped, while the docs say it is refused. `crates/validate/src/s3/document.rs:235-236` says "Anything else the root holds but whitespace and comments is refused", and `05-building-block-view.md:303` and the test header at `s3_client_smoke_verbs.rs:83-85` say the same. `document.rs:248` skips every non-element, so `<DeleteResult><?x y?></DeleteResult>` comes back as an empty result. A test pins that skipping (`s3_client_smoke_verbs.rs:2912`), while `crates/validate/src/s3/response.rs:481-482` calls a processing instruction "no part of an S3 document". The sign-off's first item was "do not skip unknown children". Fix: refuse it and replace the test, which fits that item, or correct the three sentences.

- NEEDS-HUMAN [human] — The 64 KiB budget refuses ordinary bulk deletes after the server has already done them. Against the real gateway, 1,000 keys of 40 bytes each returned `Err(Answered { status: 200, BodyFailed(OverBudget { received: 70115 }) })`, and the 50 keys I checked afterwards were all gone (HEAD 404). Every row is lost, including any `<Error>` row. 1,000 keys of 32 bytes fit; 800 keys of 60 bytes did not. `crates/validate/src/s3/verbs.rs:299-302` does disclose this, but "many long keys" undersells it: 40 bytes is not long, and S3 allows 1,000 keys of up to 1,024 bytes. The rubric's protocol-input rule asks for budgets sized for the worst-case encoded input, while brief (D) asks for this exact budget, so the builder followed the brief and this is not a builder error. A human should decide whether to raise the budget for this verb, or have #939 keep its batches small and state the limit.

## Claims in `check-gates.json` I would not rely on

- `C4-verify` shows `pass`, but its log (`gate-logs/C4-verify.log`) says "PASS (green-only)": the gate ran no red at all. Its stated reason, that the patch has no separate `tests/*.rs`, doesn't fit a patch that changes `crates/validate/tests/s3_client_smoke_verbs.rs`, and the brief predicted exit 77 here. So the row is not red→green evidence. My own reruns below fill that gap.
- `C5-mutants` shows `fail`, which overstates the problem. The log has three timeouts and no missed mutants. I reran two of the three (the `Ok("xyzzy")` return and the guard set to `false`) with a longer limit: all 47 tests fail by assertion in about 62 s, against the gate's 63 s limit. That is a slow failure, not a hang or a gap. By reading, the third (`||` to `&&`) refuses every scheme too, so it should behave the same.
- `T4-batch-review` reports 0 blocking in 25.8 s for "3x codex" on a 3,162-line diff, after one blocking finding in the last round. `review-batch.md` isn't in my inputs, so I can't say whether that review was real or reused. I make no claim either way.

## Attempted to refute, could not

- **The reds.** All six named reds go red in the expected tests:
  - Red 1 (a timeout replaces the 200): `a_probe_answered_200_keeps_its_200…`.
  - Red 3 (SDK resolver back): four tests, including `bucket_probes_send_every_name…`. Simply deleting the `.endpoint_resolver(PathStyle)` line does not compile (`-D dead-code`), so I swapped in `DefaultResolver`.
  - Red 5 (error rows dropped): three tests, including `a_mixed_delete_result…`.
  - Red 6 (unsent reported as `NoResponse`): `a_request_stopped…` and its #955 peer.
  - Red 16 (timeout becomes `NoResponse`): both probe deadline tests.
  - Red 17 (wrong `uploadId`): three tests.
- **The sign-off items.** Skipping unknown children or stray text, accepting an empty `<Key>`, hiding keys with no row, adding a `<CreateBucketConfiguration>`, sending bytes in `UploadPart`, naming the wrong part in `CompleteMultipartUpload`, and sending `quiet=true` each turn a test red. These tests exercise the production path through the real gateway, the relay or the scripted endpoint. I found no tautologies.
- **Bucket names.**
  - NBSP, U+3000, U+0085, `\n` and `\r\n` are refused as all white space, matching the docs.
  - `\0` and U+200B are sent, percent-encoded.
  - Sweeping the URL length around 65,534 bytes showed "sent" up to the limit and `RequestNotBuilt` after it, never `NoResponse`.
- **Other inputs.**
  - A newline in `source_key` is `RequestNotBuilt`.
  - An odd upload id (`a b&c=d+e/f`) is sent correctly encoded.
  - An endpoint with user info or a fragment still reaches the right host.
  - A row with a comment inside `<Key>` is refused, and CDATA reads correctly.
  - An empty key list, or a single empty key, reports the gateway's 400.

## Smaller notes, no rebuild needed

- Two small claims are not pinned by any test: no row in `MORE_FORBIDDEN_BUCKETS` has a `~` (`crates/validate/src/s3.rs:549`), and an endpoint path of `/p//` is sent as `/p//validate/`, against "one `/` between" at `s3.rs:463` (`:536` strips only one slash). I would not spend a round on either.
- Outside this diff: the gateway's `xml_escape` (`crates/gateway-s3/src/lib.rs:3803`) leaves `\r` unescaped. `delete_objects(&["a\rb"])` deletes the key, but XML line-ending rules turn the echoed key into `a\nb`, so the client reports the key as having no row and the row as not requested. Fed an escaped `&#13;`, the client reads the key correctly, so the client is behaving properly. This belongs in its own gateway issue, not this PR.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness defects found. Reviewed endpoint resolution (`crates/validate/src/s3.rs:506`), delete-result parsing (`crates/validate/src/s3/document.rs:237`), probe dispatch (`crates/validate/src/s3/verbs.rs:343`), and request assertions (`crates/validate/tests/s3_client_smoke_verbs.rs:1008`) against the target source.
- No actionable reuse, simplification, or efficiency findings. The new verbs reuse the existing capture, bounded body-reading, and result-classification helpers (`crates/validate/src/s3/verbs.rs:327`); success-document validation reuses `whole_document` (`crates/validate/src/s3/answer.rs:351`). The diff is clean under both advisory lenses.

Validation used the supplied frozen evidence; no builds were rerun. C4-ci passed, including all 47 smoke-verb tests. C4-verify reports green-only verification. C5 reports 34 caught mutants, 14 unviable mutants, and three timeouts; it does not establish those timeout cases as caught.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Decide whether this tested client contract is sufficient for #939's smoke scenario — callers still choose a safe bucket name and judge per-key discrepancies, which determine whether the validator can safely produce meaningful verdicts (`crates/validate/src/s3/verbs.rs:32`, `crates/validate/src/s3/verbs.rs:294`, `brief.md:189`).
- [ ] `crates/validate/src/s3.rs:52-54` says a URL longer than an `http` URI allows is `RequestNotBuilt`, but `S3Client::probe` (`crates/validate/src/s3/verbs.rs:343`) **panics** when the key or upload id makes the URL too long. Failing case: `client.probe(&Probe::CreateMultipartUpload { key: "k".repeat(70_000) })` unwinds at `aws-sdk-s3-1.155.0/src/operation/create_multipart_upload.rs:370` (`expect("valid request")`, `InvalidUri(TooLong)`) instead of returning `Err`. The same panic hit `CopyObject`, `UploadPart`, `CompleteMultipartUpload` and `ListParts` with a long key, and `UploadPart` and `AbortMultipartUpload` with a long upload id. Sweeping `ListParts` key length: 65,500 bytes is `RequestNotBuilt`, 65,510 and up panic. The tests only try a long bucket name (`a_request_stopped_before_it_reaches_the_connector…`), which goes through the endpoint and is reported correctly. The inputs come from the caller, and `verbs.rs:34` says the caller chooses the upload id, so no server can trigger this and S3's own 1,024-byte key limit makes it unlikely in a real run. Fix: refuse an overlong key or upload id before calling the SDK, as `RequestNotBuilt`, with a test. `head_object` panics the same way, but that comes from #955, so it is not counted here.
- [ ] No test shows that `delete_objects` keeps its 200 when the body fails on the wire. `docs/design/architecture/05-building-block-view.md:303` says a body that "stalls, is cut, overruns its framing or runs past the budget after a 2xx head is reported beside that 2xx", and that paragraph covers bulk delete, but only stalled, trickled and cut bodies on listings and probes are tested (`crates/validate/tests/s3_client_smoke_verbs.rs:1881`, `:1964`). I changed `verbs.rs` so a body timeout in `delete_objects` returns `NoResult::NoAnswer(Timeout)`, then separately so a cut body returns `NoResponse`. Both changes drop the 200, and all 47 tests still pass. Changing its `Capture::new(false)` to `true` (`verbs.rs:317`) also passes all 47, so a chunked `304` to this POST is untested too. The production code is correct today, through `settle`; the gap is in the tests, and brief (H) says no sentence may claim more than the tests show. Fix: run `BODY_FAULTS` and the `304` case through `delete_objects`.
- [ ] A processing instruction inside `<DeleteResult>` is skipped, while the docs say it is refused. `crates/validate/src/s3/document.rs:235-236` says "Anything else the root holds but whitespace and comments is refused", and `05-building-block-view.md:303` and the test header at `s3_client_smoke_verbs.rs:83-85` say the same. `document.rs:248` skips every non-element, so `<DeleteResult><?x y?></DeleteResult>` comes back as an empty result. A test pins that skipping (`s3_client_smoke_verbs.rs:2912`), while `crates/validate/src/s3/response.rs:481-482` calls a processing instruction "no part of an S3 document". The sign-off's first item was "do not skip unknown children". Fix: refuse it and replace the test, which fits that item, or correct the three sentences.
- [x] The 64 KiB budget refuses ordinary bulk deletes after the server has already done them. Against the real gateway, 1,000 keys of 40 bytes each returned `Err(Answered { status: 200, BodyFailed(OverBudget { received: 70115 }) })`, and the 50 keys I checked afterwards were all gone (HEAD 404). Every row is lost, including any `<Error>` row. 1,000 keys of 32 bytes fit; 800 keys of 60 bytes did not. `crates/validate/src/s3/verbs.rs:299-302` does disclose this, but "many long keys" undersells it: 40 bytes is not long, and S3 allows 1,000 keys of up to 1,024 bytes. The rubric's protocol-input rule asks for budgets sized for the worst-case encoded input, while brief (D) asks for this exact budget, so the builder followed the brief and this is not a builder error. A human should decide whether to raise the budget for this verb, or have #939 keep its batches small and state the limit.
- [ ] **The rejection criterion can pass for the wrong multipart operation.** Brief lines 27–30 require six multipart probes to return `501 NotImplemented`, but the target rejects any recognized bucket subresource through the same branch (`crates/gateway-s3/src/lib.rs:1797–1802`) and any recognized object subresource through another shared branch (`:1836–1847`). A `ListParts` probe accidentally sending `ListMultipartUploads` can therefore satisfy that outcome. Criterion G (brief lines 58–60) supplies success responses but does not require checking the request that elicited them. Require captured method/path/query/header/body assertions for each probe, so a wrong operation or missing operation parameter cannot pass merely by hitting a rejection guard.
- [ ] **Criterion B contradicts the existing timeout contract.** Brief lines 37–39 say a request that reached the connector and got no answer “stays `NoResponse`.” The target explicitly reports SDK operation timeouts as `Timeout { phase: Operation }` and connector timeouts as `Timeout { phase: Connect }` (`crates/validate/src/s3.rs:346–355`); these also receive no answer. Qualify the criterion to preserve typed deadline failures, reserve `NoResponse` for non-timeout transport failures, and require probe assertions for both deadline phases. Otherwise the stated acceptance rule permits losing the measured timeout phase.
- [ ] **The mandatory salvage prerequisite is unresolved in the supplied material.** Brief lines 7–9 require taking the prior patch's hunks unchanged and forbid re-deriving the design; lines 67, 89–95 and 135–137 also depend on its exact architecture paragraph, implementation and SDK evidence. Neither the review cwd nor `$PDCA_TARGET` contains `results/issue_938/iteration-v3/patch.diff` or its `build-notes.md`; `notes.json` and `sources/` are also absent. This does not prove those artifacts are missing elsewhere, but their availability and the claimed prior red/green evidence cannot be established here. Revise the brief to make the required salvage an explicitly supplied, resolvable input, or include the necessary contract and remove the exclusive instruction to transplant unavailable hunks. Include the relevant tracker excerpts so the bucket-safety constraint and prior-review disposition can be checked.
- [x] size backstop — this slice is behaving oversized: patch is 157 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep the routing, probe and row-reading code; it held up. Do not split: the size warning is mostly the test file. Fix these only: 1. S3Client::probe (verbs.rs:343) panics when a key or upload id makes the URL too long, though s3.rs:52-54 promises RequestNotBuilt. Refuse an overlong key or upload id before calling the SDK, as RequestNotBuilt, and test it. head_object's same panic is #955's and out of scope. 2. No test shows delete_objects keeping its 200 when the body fails. Run BODY_FAULTS and the chunked 304 case through delete_objects, so that returning NoAnswer on a body timeout or cut body, or Capture::new(true) at verbs.rs:317, turns a test red. 3. A processing instruction inside <DeleteResult> is skipped (document.rs:248), though document.rs:235-236, 05-building-block-view.md:303 and the test header at s3_client_smoke_verbs.rs:83-85 say it is refused. Refuse it and replace the test at s3_client_smoke_verbs.rs:2912. 4. Keep the 64 KiB budget, as brief (D) asks. Replace "many long keys" (verbs.rs:299-302) and state the limit in the docs with real numbers: 1,000 keys of 40 bytes overran it (70,115 bytes) after the server had deleted them, and every row was lost; 1,000 keys of 32 bytes fit.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up: gateway `xml_escape` (`crates/gateway-s3/src/lib.rs:3803`) leaves `\r` unescaped, so a deleted key `a\rb` echoes back as `a\nb` and looks like a missing row plus an unrequested one (issue_956 adversary).
- Follow-up: `head_object` panics instead of returning `RequestNotBuilt` when a long key makes the URL too long; same crash as #956's `probe` fix, from #955's code, so out of scope for #956.
