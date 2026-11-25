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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 97.9% — 280 of 286 instrumentable changed lines executed (floor 80%); 286 of 668 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 64 mutants tested in 17m: 47 caught, 14 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_956/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.74s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No patch defect found in #956’s bulk S3 deletes, twelve operation probes and endpoint routing; prior-art confirmation and fitness for the downstream runner remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The observable contract distinguishes unsent requests, unanswered requests and body failures after a response, with explicit request forms and caller-owned judgment; `brief.md:41`, `brief.md:116`, `crates/validate/src/s3/document.rs:72`. |
| C2 Reproduction (red pre-fix) | N/A | Net-new APIs have no executable pre-fix symptom; stashing production while retaining tests independently reproduced missing imports/methods, exactly the declared compile-only limitation (`brief.md:179`, `reviewer-red-green.log:7`). |
| C3 Change | PASS | The permitted client surface and latest carry-forward corrections are present, including the reserved refused port and corrected response-body claims; `crates/validate/tests/s3_client_smoke_verbs.rs:1682`, `crates/validate/src/s3.rs:23`, `docs/design/architecture/05-building-block-view.md:303`. |
| C4 Verification (red→green) | PASS | Under the declared net-new posture, independent validation passed all 192 crate tests and restored 53/53 smoke tests after the compile-only red; no behavioral pre-fix red is claimed (`reviewer-green.log:269`, `reviewer-red-green.log:339`, `reviewer-red-green.log:418`). |
| C5 Causal adequacy | PASS | Replacing SDK name interpretation addresses the routing cause directly; exact request assertions and strict result accounting exercise the lost-evidence cases, with no capability-probe workaround (`crates/validate/src/s3.rs:494`, `crates/validate/src/s3/document.rs:238`, `crates/validate/tests/s3_client_smoke_verbs.rs:945`). |
| T1 Structure | PASS | Shared capture, framing and XML readers preserve the client boundary and avoid another response-reading implementation; normal dependencies remain third-party only (`crates/validate/src/s3/verbs.rs:378`, `crates/validate/src/s3/answer.rs:351`, `crates/validate/Cargo.toml:24`). |
| T2 Shape | PASS | Typed results preserve document order, missing requested keys and unexpected rows without taking over #937’s judgment; living architecture explains that boundary and the budget (`crates/validate/src/s3/document.rs:80`, `crates/validate/src/s3/document.rs:106`, `docs/design/architecture/05-building-block-view.md:303`). |
| T3 Runtime | PASS | Real loopback runs exercise deadlines, post-head failures and the 41/42-error budget boundary; the repaired refused-port case passed 140 runs across four processes (`crates/validate/tests/s3_client_smoke_verbs.rs:681`, `crates/validate/tests/s3_client_smoke_verbs.rs:1782`, `reviewer-port-stress.log:1`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent at Check; their substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`); the frozen batched review separately reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm acceptance of the recorded affected-path prior-art check — merged, closed-unmerged and rejected work are named in `brief.md:207`, but the supplied repository has only one synthetic commit and no search transcript, so completeness cannot be independently settled (`reviewer-prior-art.log:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this client contract for #939’s runner — safe bucket-name selection and delete batch sizing remain the caller’s responsibility, and an over-budget reply loses rows after the server acts (`crates/validate/src/s3/verbs.rs:37`, `crates/validate/src/s3/verbs.rs:348`, `brief.md:189`). |

Source citations above resolve in `$PDCA_TARGET`; brief and evidence-log citations resolve in this review directory. The target matches the supplied patch: reverse-apply checking passed, and the stash/pop check restored the original diff byte for byte (`reviewer-scanners.log:10`, `reviewer-red-green.log:356`).

The latest carry-forward findings are resolved. The refused connection now retains a bound, non-listening socket through the relay check, preventing another test from claiming its port. Its stress run passed 140/140; this establishes the observed result, not a guarantee under every host load. The documentation now distinguishes single-object PUT/DELETE from probes, includes POST in the 304 framing rule, and states the delete-result refusal directly (`crates/validate/tests/s3_client_smoke_verbs.rs:1682`, `docs/design/architecture/05-building-block-view.md:303`). Earlier settled size and scope decisions are not reopened.

Independent checks passed: `cargo test -p wyrd-validate` (192 tests), the restored smoke suite (53), `cargo clippy -p wyrd-validate --all-targets -- -D warnings`, spelling, formatting, docs lint and the renderer/link audit of 101 pages (`reviewer-green.log:269`, `reviewer-red-green.log:418`, `reviewer-clippy.log:220`, `reviewer-scanners.log:1`, `reviewer-docs.log:2`). Both named external dependencies, `typos` and the docs renderer, were exercised; these checks used the actual SDK, gateway and loopback fixtures required by the brief.

The broader harness results are log-adjudicated, not independently rerun: workspace CI completed successfully (`gate-logs/C4-ci.log:4659`), TiKV/server feature clippy completed (`gate-logs/host-tikv.log:211`), and coverage measured 280/286 instrumentable changed lines, 97.9%, with 382 changed lines unscored (`gate-logs/C4-diff-cov.log:296`). The verification wrapper explicitly reports green-only (`gate-logs/C4-verify.log:60`); the independent stash test adds a compile failure, not the six targeted behavioral mutation demonstrations named in the brief.

The mutation row’s failure is not evidence of a surviving patch defect. Its captured result is 47 caught, 14 unviable and three timeouts, with no reported survivors; the timed-out variants corrupt endpoint resolution (`gate-logs/C5-mutants.log:13`). Those timeouts remain inconclusive mutation outcomes. The advisory C5 verdict rests on the reviewed cause and exercised contracts, not on treating that row as green.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 956, iteration 5

I could not break the routing, the probes or the row reading. Four narrow findings remain: two doc sentences that claim too much, one claim no test backs, and one test that can still cause failures on a busy host. A fifth item asks the human to check the T4 gate. I ran everything on a scratch copy of `$PDCA_TARGET`, using the host's `cargo`.

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/response.rs:45-46` still says "A PUT or DELETE success has no body at all, so one that sends a byte is refused at that byte", in the docs of the public `BUFFERED_BODY_BUDGET`. `response.rs:13-14` says the same ("a PUT's or a DELETE's success at nothing, since S3 sends neither with a body"). This diff makes both false: `Probe::CopyObject` is a PUT, and its `200` with `<CopyObjectResult>` is a success (`crates/validate/src/s3/verbs.rs:165`, pinned at `crates/validate/tests/s3_client_smoke_verbs.rs:4651`). Round 4 corrected this sentence in `s3.rs:27-31` and in the architecture doc, but not at its source, and brief (H) covers "the code's docs". The list at `response.rs:44` also leaves out probes and delete results. Fix: limit these sentences to the PUT and DELETE of one object, as `s3.rs` now does. No code changes.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/document.rs:75-76` says a row's key is compared "exactly as sent once the document's XML escapes are read". That is false for a key holding a carriage return. Failing case against the real gateway: put `cr\rkey`, then call `delete_objects(&["cr\rkey"])`. The client sends `cr&#xD;key`, the gateway deletes the object (HEAD goes 200 → 404) and writes a raw CR in its row, because `xml_escape` (`crates/gateway-s3/src/lib.rs:3803`) does not escape it. The XML parser turns that CR into `\n`. The result then says `rows=[Deleted{key:"cr\nkey"}]`, `keys_without_a_row=["cr\rkey"]` and `rows_not_requested=[Deleted{key:"cr\nkey"}]`, so a key that was deleted looks unaccounted for. `crlf\r\nkey` does the same; `lf\nkey` is fine. The client reads the XML correctly, so the root cause is the gateway, which is out of scope here. In this diff, reword the doc sentence to name line-ending normalisation (XML turning a raw CR or CRLF into LF) and pin the case with a test. Open a separate issue for the gateway.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3.rs:31-32` and `docs/design/architecture/05-building-block-view.md:303` say the 64 KiB budget caps "a probe's" body, but no test sends a probe an over-budget body. The budget cases at `crates/validate/tests/s3_client_smoke_verbs.rs:2613-2660` run only the listing and delete verbs, and `BODY_FAULTS` (`:2133`) has no over-budget fault. I confirmed this with a mutation at `crates/validate/src/s3/verbs.rs:475`: turning a probe's `BodyFailed(OverBudget)` into `Err(NoResponse)`, which loses the 200. All 53 tests still passed. Brief (H) forbids a claim the tests do not show, and the case can happen: S3 can answer `ListBuckets` or `ListParts` with more than 64 KiB, and that conforming success comes back as a `200` beside `OverBudget`. Fix: add an over-budget leg and an exactly-the-budget leg for a document probe.

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:657-671` does 1,000 PUTs and 1,000 HEADs on fresh connections, which leaves 1,971 sockets in `TIME_WAIT` from this one test. The other 52 tests in the file leave about 745 in total. A full run costs about 2,700 of this host's 28,232 ephemeral ports (range 32768-60999), so about 10 runs a minute, across all jobs on the host, use them all. After that, `bind("127.0.0.1:0")` fails at `:1882`, `:1754` and in the relay. With 20 parallel loops, 152 of 200 runs failed, and every panic message was `AddrInUse`. That load is extreme, but it is the same "several copies on a loaded host" condition behind round 4's flake, and this cause remains after that fix. The test's own earlier block already samples with `step_by(50)` (`:646`), and sampling the PUTs and HEADs here would still show that the keys are gone. The figures the docs pin come from the `Content-Length`, so they do not need all 1,000 objects to exist.

- NEEDS-HUMAN [human] — `gate-logs/T4-batch-review.log:10` is the only evidence for the gating T4 row in `check-gates.json:104-112`, which claims a "3x codex" multi-pass review. It is one summary line, and the row took 27.29 s for a 197 KB diff. That is very quick for three deep passes, and the log shows no pass count. I cannot tell whether the passes really ran or the result was cached or empty, because `review-batch.md` is not among my inputs. This is a question about the gate, not evidence against the fix. Please check `results/issue_956/review-batch.md` before relying on "0 blocking".

## Evidence and gates

- `gate-logs/C4-verify.log:60-62` is not a red→green proof. The row says `pass`, but the log says "PASS (green-only)", and the brief had predicted UNVERIFIABLE (exit 77). I ran the required reds myself on `crates/validate/src/s3.rs` and `crates/validate/src/s3/{verbs,document}.rs`. Each one failed the tests the brief names, and the unmodified copy passed 53 of 53:
  - Red 1 (a probe returns only the timeout): `a_probe_answered_200…` failed.
  - Red 3 (the SDK's default endpoint rules): 4 tests failed, including the bucket-name and region tests.
  - Red 5 (drop `<Error>` rows): 4 tests failed.
  - Red 6 (unsent reported as `NoResponse`): 3 tests failed.
  - Red 16 (a deadline reported as `NoResponse`): both probe deadline tests failed.
  - Red 17 (wrong `uploadId`): 3 tests failed.
- The tests exercise the production client, not a copy of it. They call the real `S3Client` against the real gateway, the relay or the scripted endpoint. These further mutations were also caught: `UploadPart` with bytes, `CreateBucket` with a configuration, a wrong part number in `CompleteMultipartUpload`, a copy source without its bucket, `quiet` delete, `Capture::new(true)` on bulk delete, processing instructions no longer refused, no trailing-slash strip on the endpoint path, the `/` discount in `too_long`, and lower-case hex.
- `gate-logs/C5-mutants.log:13-15` is a false alarm for this diff. I ran the `Ok("xyzzy")` mutant on each test binary. `s3_client_smoke_verbs` fails 52 of 53 tests within about 61 s. The hang is in `s3_client_nonconforming_responses`, an existing file this diff does not touch. So the mutant is caught, and the timeout comes from that older test file. I ran only this mutant. The other two change the same scheme check at `s3.rs:515`, and I expect the same result but did not run them.
- Round 4's flake fix holds. The refused-connection leg (`crates/validate/tests/s3_client_smoke_verbs.rs:1682-1701`) keeps its port held by a bound socket that is not listening, and no test in the file frees a port any more. 60 of 60 runs passed with 6 parallel loops under 40 CPU hogs. To stay within the port limit, those runs skipped the three tests whose names contain `a_bulk_delete`, so they do not show that those three are free of timing flakes.

## Tried and could not break

- **Bucket names on the client itself:** `--x-s3`, `--xa-s3`, `--op-s3`, `arn:` and `a/b` as the client's own bucket, on both IP and host-name endpoints, through all 12 probes plus `DeleteObjects`, HEAD, `ListObjectsV2` and DELETE. All 192 requests were exactly one request, signed for `s3`, sent to the configured host.
- **The overlong-key guard (`crates/validate/src/s3/verbs.rs:131-159`):** I read the SDK's encoding set, and it matches `path_segment` exactly. No panic turned up.
- **Inputs that cannot be sent:** an empty or white-space key, an empty upload id, a copy source holding `\n` or NUL, and NBSP or U+3000 as a bucket name. All are `RequestNotBuilt` with nothing sent.
- **Inputs that can be sent:** `\n` and NUL in a bucket, key or upload id are percent-encoded and sent as one request.
- **Bulk delete edge cases against the gateway:** no keys, an empty key, a control character, U+FFFE, or 1,001 keys all come back as the gateway's 400 with the status kept. Duplicate keys, a 1,025-byte key and XML-special keys all read correctly.
- **Odd `<DeleteResult>` documents:** CDATA white space at the root, a row in a foreign namespace, an attribute on a row, and `<Code>` or `<Error>` nested inside a `<Deleted>` are all read as `Deleted`. The docs say a row's other fields are passed over, so I am not raising these. No real server sends them.
- **SDK citations:** the lines I spot-checked match `aws-sdk-s3` 1.155.0, `aws-smithy-runtime-api` 1.19.2, `aws-smithy-http` 0.64.3 and `http` 1.4.2. The sizing arithmetic also holds: 62,115 + 41×83 = 65,518, and + 42×83 = 65,601, on either side of the 65,536 budget.
- **Brief scope:** `lib.rs`, `Cargo.toml`, `deny*.toml` and the three older test files are not in the diff.

## Small gaps, not worth a rebuild alone

- `crates/validate/src/s3.rs:552`: taking `~` out of the unreserved set survives all 53 tests. No bucket name in `MORE_FORBIDDEN_BUCKETS` (`crates/validate/tests/s3_client_smoke_verbs.rs:161`) holds a `~`. If another round happens, add one row, such as `("a~b", "a~b")`.
- `crates/validate/src/s3.rs:194`: removing `.disable_s3_express_session_auth(true)` survives. The code comment says this is only a guard against a future SDK, so no test can show it today. I am not raising it.

## A side effect of my testing

My 20-loop run filled the host's `TIME_WAIT` table (155,490 sockets at about 13:16 host time). Other jobs that hit `AddrInUse` on this host in the following minute or so may have been caused by that run, not by their own code.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness defects found in the reviewed diff. Checked endpoint routing (`crates/validate/src/s3.rs:509`), delete-result parsing and reporting (`crates/validate/src/s3/document.rs:238`, `crates/validate/src/s3/verbs.rs:361`), probe dispatch and response handling (`crates/validate/src/s3/verbs.rs:404`), and the reserved-port regression fix (`crates/validate/tests/s3_client_smoke_verbs.rs:1687`).
- No actionable reuse, simplification, or efficiency findings. The additions reuse the existing capture, bounded body reader, response classification, and XML helpers (`crates/validate/src/s3/verbs.rs:385`, `crates/validate/src/s3/answer.rs:351`, `crates/validate/src/s3/document.rs:262`).

Validation evidence was inspected from the frozen gate logs; tests were not rerun. C4 CI passed, including all 53 smoke-verb tests, and instrumentable diff coverage was 97.9%. C4 verification established green only. C5 reported 47 caught mutants, 14 unviable, and three timeouts; those timeouts are inconclusive, not surviving mutants.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm acceptance of the recorded affected-path prior-art check — merged, closed-unmerged and rejected work are named in `brief.md:207`, but the supplied repository has only one synthetic commit and no search transcript, so completeness cannot be independently settled (`reviewer-prior-art.log:1`).
- [ ] Validation — fitness-to-purpose — Accept this client contract for #939’s runner — safe bucket-name selection and delete batch sizing remain the caller’s responsibility, and an over-budget reply loses rows after the server acts (`crates/validate/src/s3/verbs.rs:37`, `crates/validate/src/s3/verbs.rs:348`, `brief.md:189`).
- [ ] `crates/validate/src/s3/response.rs:45-46` still says "A PUT or DELETE success has no body at all, so one that sends a byte is refused at that byte", in the docs of the public `BUFFERED_BODY_BUDGET`. `response.rs:13-14` says the same ("a PUT's or a DELETE's success at nothing, since S3 sends neither with a body"). This diff makes both false: `Probe::CopyObject` is a PUT, and its `200` with `<CopyObjectResult>` is a success (`crates/validate/src/s3/verbs.rs:165`, pinned at `crates/validate/tests/s3_client_smoke_verbs.rs:4651`). Round 4 corrected this sentence in `s3.rs:27-31` and in the architecture doc, but not at its source, and brief (H) covers "the code's docs". The list at `response.rs:44` also leaves out probes and delete results. Fix: limit these sentences to the PUT and DELETE of one object, as `s3.rs` now does. No code changes.
- [ ] `crates/validate/src/s3/document.rs:75-76` says a row's key is compared "exactly as sent once the document's XML escapes are read". That is false for a key holding a carriage return. Failing case against the real gateway: put `cr\rkey`, then call `delete_objects(&["cr\rkey"])`. The client sends `cr&#xD;key`, the gateway deletes the object (HEAD goes 200 → 404) and writes a raw CR in its row, because `xml_escape` (`crates/gateway-s3/src/lib.rs:3803`) does not escape it. The XML parser turns that CR into `\n`. The result then says `rows=[Deleted{key:"cr\nkey"}]`, `keys_without_a_row=["cr\rkey"]` and `rows_not_requested=[Deleted{key:"cr\nkey"}]`, so a key that was deleted looks unaccounted for. `crlf\r\nkey` does the same; `lf\nkey` is fine. The client reads the XML correctly, so the root cause is the gateway, which is out of scope here. In this diff, reword the doc sentence to name line-ending normalisation (XML turning a raw CR or CRLF into LF) and pin the case with a test. Open a separate issue for the gateway.
- [ ] `crates/validate/src/s3.rs:31-32` and `docs/design/architecture/05-building-block-view.md:303` say the 64 KiB budget caps "a probe's" body, but no test sends a probe an over-budget body. The budget cases at `crates/validate/tests/s3_client_smoke_verbs.rs:2613-2660` run only the listing and delete verbs, and `BODY_FAULTS` (`:2133`) has no over-budget fault. I confirmed this with a mutation at `crates/validate/src/s3/verbs.rs:475`: turning a probe's `BodyFailed(OverBudget)` into `Err(NoResponse)`, which loses the 200. All 53 tests still passed. Brief (H) forbids a claim the tests do not show, and the case can happen: S3 can answer `ListBuckets` or `ListParts` with more than 64 KiB, and that conforming success comes back as a `200` beside `OverBudget`. Fix: add an over-budget leg and an exactly-the-budget leg for a document probe.
- [ ] `crates/validate/tests/s3_client_smoke_verbs.rs:657-671` does 1,000 PUTs and 1,000 HEADs on fresh connections, which leaves 1,971 sockets in `TIME_WAIT` from this one test. The other 52 tests in the file leave about 745 in total. A full run costs about 2,700 of this host's 28,232 ephemeral ports (range 32768-60999), so about 10 runs a minute, across all jobs on the host, use them all. After that, `bind("127.0.0.1:0")` fails at `:1882`, `:1754` and in the relay. With 20 parallel loops, 152 of 200 runs failed, and every panic message was `AddrInUse`. That load is extreme, but it is the same "several copies on a loaded host" condition behind round 4's flake, and this cause remains after that fix. The test's own earlier block already samples with `step_by(50)` (`:646`), and sampling the PUTs and HEADs here would still show that the keys are gone. The figures the docs pin come from the `Content-Length`, so they do not need all 1,000 objects to exist.
- [ ] `gate-logs/T4-batch-review.log:10` is the only evidence for the gating T4 row in `check-gates.json:104-112`, which claims a "3x codex" multi-pass review. It is one summary line, and the row took 27.29 s for a 197 KB diff. That is very quick for three deep passes, and the log shows no pass count. I cannot tell whether the passes really ran or the result was cached or empty, because `review-batch.md` is not among my inputs. This is a question about the gate, not evidence against the fix. Please check `results/issue_956/review-batch.md` before relying on "0 blocking".
- [ ] **The rejection criterion can pass for the wrong multipart operation.** Brief lines 27–30 require six multipart probes to return `501 NotImplemented`, but the target rejects any recognized bucket subresource through the same branch (`crates/gateway-s3/src/lib.rs:1797–1802`) and any recognized object subresource through another shared branch (`:1836–1847`). A `ListParts` probe accidentally sending `ListMultipartUploads` can therefore satisfy that outcome. Criterion G (brief lines 58–60) supplies success responses but does not require checking the request that elicited them. Require captured method/path/query/header/body assertions for each probe, so a wrong operation or missing operation parameter cannot pass merely by hitting a rejection guard.
- [ ] **Criterion B contradicts the existing timeout contract.** Brief lines 37–39 say a request that reached the connector and got no answer “stays `NoResponse`.” The target explicitly reports SDK operation timeouts as `Timeout { phase: Operation }` and connector timeouts as `Timeout { phase: Connect }` (`crates/validate/src/s3.rs:346–355`); these also receive no answer. Qualify the criterion to preserve typed deadline failures, reserve `NoResponse` for non-timeout transport failures, and require probe assertions for both deadline phases. Otherwise the stated acceptance rule permits losing the measured timeout phase.
- [ ] **The mandatory salvage prerequisite is unresolved in the supplied material.** Brief lines 7–9 require taking the prior patch's hunks unchanged and forbid re-deriving the design; lines 67, 89–95 and 135–137 also depend on its exact architecture paragraph, implementation and SDK evidence. Neither the review cwd nor `$PDCA_TARGET` contains `results/issue_938/iteration-v3/patch.diff` or its `build-notes.md`; `notes.json` and `sources/` are also absent. This does not prove those artifacts are missing elsewhere, but their availability and the claimed prior red/green evidence cannot be established here. Revise the brief to make the required salvage an explicitly supplied, resolvable input, or include the necessary contract and remove the exclusive instruction to transplant unavailable hunks. Include the relevant tracker excerpts so the bucket-safety constraint and prior-review disposition can be checked.
- [ ] size backstop — this slice is behaving oversized: patch is 192 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep the routing, probe and row-reading code; it held up. Do not split: the human overrode the size warning again, since this round's findings are narrow. Fix these only: 1. crates/validate/src/s3/response.rs:13-14 and :45-46 say a PUT or DELETE success has no body, which is false for Probe::CopyObject (crates/validate/src/s3/verbs.rs:165). Limit both sentences to the PUT and DELETE of one object, as s3.rs now does, and add probes and delete results to the list at response.rs:44. This is a docs change to response.rs, allowed by brief (H). No code changes. 2. crates/validate/src/s3/document.rs:75-76 says a row's key is compared "exactly as sent once the document's XML escapes are read", which is false for a key holding a carriage return. Against the real gateway, delete_objects(&["cr\rkey"]) deletes the object but reports the key under keys_without_a_row and a "cr\nkey" row under rows_not_requested; "crlf\r\nkey" does the same. Reword the sentence to name line-ending normalisation (XML turning a raw CR or CRLF into LF) and pin the case with a test. The root cause is in the gateway (crates/gateway-s3/src/lib.rs:3803) and is out of scope: do not change the gateway. 3. crates/validate/src/s3.rs:31-32 and docs/design/architecture/05-building-block-view.md:303 say the 64 KiB budget caps a probe's body, but no test shows it. Turning a probe's BodyFailed(OverBudget) into Err(NoResponse) at verbs.rs:475 passes all 53 tests. Add an over-budget leg and an exactly-the-budget leg for a document probe. 4. crates/validate/tests/s3_client_smoke_verbs.rs:657-671 does 1,000 PUTs and 1,000 HEADs on fresh connections, leaving 1,971 sockets in TIME_WAIT, and can exhaust ephemeral ports (AddrInUse) when many runs share a host. Sample the PUTs and HEADs, as the earlier block does with step_by(50) at :646. The sizes the docs pin come from the Content-Length, so they do not need all 1,000 objects to exist. Nothing else changes.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
