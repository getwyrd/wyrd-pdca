# Result — issue 741 / validate-s3-client-layer

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: (framed as the gap) `crates/validate` after #740 parses arguments and echoes
  them; it cannot speak to an endpoint at all. The client layer was implicit in the original
  slicing and therefore in nobody's brief, which is why #741 exists as a named slice.
- Success criterion: BINDING (demonstrable by C4-verify at Check, in-process, no
  container, no live cluster): against a Wyrd S3 gateway served in-process over a loopback
  listener from the crate's own dev-dependencies —
  1. one object **round-trips** PUT → GET → DELETE through the client layer, byte-identical
     on the way back, and a subsequent GET reports the typed not-found error;
  2. an **error response surfaces as a typed value** carrying its HTTP status, its S3
     `<Code>`, and the `x-amz-request-id` the gateway stamps
     (`crates/gateway-s3/src/lib.rs:1552-1557`) — asserted field by field, never by
     substring-matching a `Display` string;
  3. a **multi-chunk object streams, in BOTH directions, each with its own
     aggregation-sensitive oracle** — not a process RSS measurement, and not an assertion
     that merely *permits* a buffering implementation. Route the fixture's traffic through a
     tiny in-process loopback **interposer** (a TCP relay between the SDK client and the
     gateway's listener that counts bytes per direction and can hold the tail back); it is
     dev-only test code, and it is what makes "the client did not aggregate" observable:
     * **PUT** — the payload is produced by a generator that is never fully materialised.
       When the generator is asked for its FINAL chunk, the interposer must ALREADY have
       forwarded bytes to the gateway (assert `> 0`, with a payload large enough — tens of
       MiB — that no socket or SDK window can account for the whole body). A client that
       buffers the body before sending forwards nothing until the generator is exhausted, so
       this fails.
     * **GET** — the interposer forwards a prefix of the response and then WITHHOLDS the
       tail. The client must yield at least one body chunk to the test while the tail is
       withheld; only then is the tail released and the read completed, asserting
       byte-identity with what was PUT. A client that collects the whole response before
       yielding produces nothing during the hold and fails (as a timeout with a stated
       message, not a hang — bound the wait).
     Do MUST demonstrate both mutations: temporarily buffer the PUT body, record that the
     PUT assertion fires; temporarily collect the GET body, record that the GET assertion
     fires; revert both, and report both in `build-notes.md`. If Do finds a cleaner oracle
     that is equally aggregation-sensitive in both directions, it may substitute it — and
     must say in `build-notes.md` what it is and why it is decisive. A weaker oracle (chunk
     counts, largest-buffer-seen alone) is NOT an acceptable substitute for the GET leg: the
     "collect, then re-chunk small" implementation passes it.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) `aws-sdk-s3` wiring against an arbitrary `--endpoint`: path-style
  addressing, explicit region and `s3` service scope, static credentials from #740's
  resolution, no anonymous path, retries and stalled-stream protection off by default so a
  failure is reported rather than papered over; (b) SigV4 header auth over plain HTTP (see
  Impact — the TLS limit is stated, not fixed here); (c) a typed error surface mapping S3
  XML error bodies onto values carrying status, `Code` and `x-amz-request-id`; (d) streaming
  in both directions — a PUT chunked as it is generated, a GET read incrementally; (e) the
  ADR-0003 §2 three-test dependency audit written into `build-notes.md`, and the
  RUSTSEC-2026-0253 waiver rationale in `deny.toml` corrected (see Design — it currently
  asserts something this slice makes false).
  **/ out of scope, and deliberately so:** the **5 GiB memory property** from the issue's
  definition of done — the maintainer's own review calls this slice oversized and names that
  leg as the one to split out (issue #741 comment, 2026-08-16), it is blocked on #635, and
  its home is #761, whose definition of done already says "Resident set stays flat
  across a 5 GiB transfer"; a TLS client (blocked on a rustls crypto-provider
  license decision, `crates/gateway-s3/src/lib.rs:50-57`); the capability matrix and its
  expected-failure assertions (#743); the oracle, payload derivation and size classes
  (#744, #746); workflows, pools, scenarios, verdict; MinIO and the corrupting proxy (#751,
  #758); any change to the Wyrd gateway itself; **any edit under `crates/server/`**,
  including deduplicating its inline `aws-sdk-s3` dev pin into `[workspace.dependencies]` —
  see Impact, that cleanup is a follow-up and touching it here would break this bundle's
  file-set disjointness from #738 in the same wave.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 89 mutants tested in 3m: 14 missed, 19 caught, 56 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_741/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.01s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #741: add a streaming, authenticated S3 client with typed errors to `wyrd-validate`; two independently reproduced error-mapping defects require correction.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The client-layer milestone has falsifiable round-trip, typed-error and bidirectional streaming criteria, with HTTP transport and the later 5 GiB milestone explicitly bounded; `brief.md:38`, `brief.md:161`, `brief.md:178`. |
| C2 Reproduction (red pre-fix) | PASS | Reproduced the declared criterion-absence red: stashing production changes leaves the new test unable to import the client API; this establishes absence, not a failing pre-existing behavioral assertion (`reviewer-red.log:3`, `brief.md:185`). |
| C3 Change | PASS | The change stays within the client, dependencies and living architecture surfaces; the existing locked SDK/lru versions satisfy the waiver-removal trigger, so retiring the waiver is justified rather than scope expansion (`crates/validate/src/lib.rs:23`, `Cargo.lock:210`, `Cargo.lock:2139`, `docs/design/architecture/05-building-block-view.md:253`, `brief.md:369`). |
| C4 Verification (red→green) | FAIL | Four supplied tests pass after restoration, but two independently compiled public-API probes fail the required error distinctions; the green suite therefore does not establish the whole contract (`reviewer-restored-green.log:278`, `reviewer-probes.log:21`, `reviewer-probes.log:27`, `brief.md:284`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Correct and regression-test the SDK-to-wire error interpretation — treating all SDK service metadata as received S3 facts fabricates an absent code and loses unreadable-response diagnostics (`crates/validate/src/client.rs:241`, `crates/validate/src/client.rs:249`, `reviewer-probes.log:21`, `reviewer-probes.log:27`). |
| T1 Structure | PASS | The blackbox dependency boundary remains intact and client I/O is separate from later capability decisions; the independent blackbox guard passed (`crates/validate/Cargo.toml:23`, `crates/validate/Cargo.toml:38`, `crates/validate/src/lib.rs:12`, `reviewer-ci.log:29`). |
| T2 Shape | FAIL | Fixture lifetime does not own gateway/relay/pump lifetime: dropping their task handles detaches helpers instead of aborting them on fixture drop, contrary to the standing rubric (`crates/validate/tests/s3_client_roundtrip.rs:102`, `crates/validate/tests/s3_client_roundtrip.rs:175`, `crates/validate/tests/s3_client_roundtrip.rs:180`, `AGENTS.md:181`). |
| T3 Runtime | FAIL | A bare 404 reports `Code("NotFound")`, and a non-XML 500 reports `Service(NoCodeInBody)`; downstream capability assertions receive invented or misclassified facts (`crates/validate/src/client.rs:249`, `crates/validate/src/client.rs:254`, `reviewer-probes.log:21`, `reviewer-probes.log:27`). |
| T4 Contribution | FAIL | The recorded rubric review still has unresolved findings, independently grounded below; affected-path prior art was checked, while the contribution-artifact audit is separately N/A until its mandatory publish rerun (`gate-logs/T4-batch-review.log:10`, `reviewer-prior-art.log:1`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Add shipped deadline-expiry, truncated-GET and failing/length-mismatched PUT regressions — the new bounded-wait and integrity guarantees currently lack those tests, and relevant guard-removal mutants survive (`crates/validate/src/client.rs:224`, `crates/validate/src/client.rs:320`, `crates/validate/src/client.rs:393`, `gate-logs/C5-mutants.log:18`, `gate-logs/C5-mutants.log:20`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm this HTTP-only client-layer milestone, its declared criterion-absence red, and the session-recorded shipped-SDK acceptance at sign-off — deployed TLS topology and the 5 GiB memory property remain outside this slice (`brief.md:17`, `brief.md:185`, `brief.md:196`, `brief.md:430`). |

The advisory conclusion rests on two observed runtime defects, a fixture-lifetime convention violation, and missing boundary regression coverage. The streaming design itself passed both independent aggregation mutations. Source citations above and below resolve in the supplied `$PDCA_TARGET`; the patch matched that target, and its original tracked/untracked changes were restored after the red leg. No builder rationale was read and no implementation fix was made.

1. **FAIL — preserve the absence of an S3 error code.** At `crates/validate/src/client.rs:249`, SDK metadata takes precedence over the already-computed body-presence fact. A real loopback reply `HTTP/1.1 404 Not Found` with `Content-Length: 0` and `x-amz-request-id: reviewer-wire-id` produces `Service { status: 404, code: Code("NotFound"), request_id: Some("reviewer-wire-id") }`. The public `ErrorCode` contract says `Code` came from the body (`crates/validate/src/error.rs:61`); this reply has none. Preserve `NoBody` and add a wire-level regression. Observed failure: `reviewer-probes.log:21`.

2. **FAIL — preserve unreadable-response failures.** At `crates/validate/src/client.rs:241`, every SDK `ServiceError` becomes a structured S3 service response, while `Unreadable` is reserved for the SDK's separate `ResponseError` arm. A real HTTP 500 carrying `this is not XML` instead yields `Service { code: NoCodeInBody, message: None }`, losing the distinction and diagnostics promised at `crates/validate/src/error.rs:23` and `brief.md:286`. Account for SDK parse failures wrapped in service errors, without rejecting valid unknown S3 codes: the control reply with `<Code>FutureExtension</Code>` correctly remains a service error (`reviewer-probes.log:27`, `reviewer-probes.log:32`).

3. **FAIL — retain and abort fixture helper tasks.** `start_gateway` discards its server handle; `start_relay` discards the listener handle and both per-connection pump handles (`crates/validate/tests/s3_client_roundtrip.rs:102`, `crates/validate/tests/s3_client_roundtrip.rs:175`). Dropping `Relay` neither closes the listener task nor cancels pumps waiting on socket I/O. Per-test runtime teardown eventually removes them, but fixture drop itself does not meet `AGENTS.md:181`. Give the fixture ownership of all spawned helpers and abort them when it drops; do not join an infinite accept loop.

4. **NEEDS-HUMAN [impl] — ship tests for the new failure boundaries.** The four added tests exercise valid transfers, ordinary S3 errors, connection refusal and streaming order. They do not expire either configurable deadline, truncate a successful GET, or supply short/oversized/failing PUT streams. The frozen mutant run confirms survivors in timeout classification and length enforcement (`gate-logs/C5-mutants.log:18`, `gate-logs/C5-mutants.log:20`). Independent short-PUT, extra-chunk PUT and zero-length/nonempty-source probes all returned `S3Error::Body` correctly (`reviewer-probes.log:38`); this is a regression-coverage defect, not a claim that those three production paths currently fail. Some other missed mutants are equivalent, such as deleting the dispatch-failure arm when the fallback produces the same transport variant; the count alone is not a defect.

The independent evidence supports the successful transfer and streaming claims, with these precise limits:

| Evidence | Result | Basis |
|----------|--------|-------|
| Pre-fix / restored target | PASS for declared absence / green | A path-scoped stash retained the new test and removed production changes; compilation failed on absent symbols/dependencies. After stash pop, all four tests passed (`reviewer-red.log:3`, `reviewer-red.log:285`, `reviewer-restored-green.log:278`). No behavioral pre-fix assertion is claimed. |
| PUT aggregation mutation | PASS — mutation rejected | In a scratch copy of the client and the supplied fixture, collecting the input before sending caused the existing assertion to observe **0** forwarded bytes at the final piece (`reviewer-streaming-put-buffered.log:7`). |
| GET aggregation mutation | PASS — mutation rejected | Independently collecting the response before returning caused the existing withheld-tail test to fail at its **15-second** bound (`reviewer-streaming-get-collected.log:7`). The unmodified scratch baseline passed all four tests; both mutations were confined to scratch and reverted. |
| C4-ci | PASS from frozen log; partial independent rerun | The frozen run includes the four client tests, deny checks, conformance and completion (`gate-logs/C4-ci.log:2834`, `gate-logs/C4-ci.log:3292`, `gate-logs/C4-ci.log:3893`). Independently, typos, docs lint/render, gitlink, unsafe and blackbox guards passed; the rerun then stopped because this host lacks `cargo-fmt` for its stable toolchain (`reviewer-ci.log:13`, `reviewer-ci.log:33`). That host limitation is not a patch defect. |
| C4-verify | NEEDS-HUMAN — declared absence posture | The frozen log shows four green tests followed by compile-only pre-fix failure, exactly as independently reproduced; sign-off must accept the explicitly declared new-API proof posture (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:277`, `brief.md:185`). |
| C4-diff-cov | N/A — base-state caveat | The scanner never measured coverage because the stacked patch did not apply to `origin/main`; this is not evidence of a compilation or coverage defect in the supplied, matching target (`gate-logs/C4-diff-cov.log:10`). Coverage remains unmeasured. |
| C5-mutants | FAIL — advisory evidence | Frozen output records 89 tested: 14 missed, 19 caught, 56 unviable. The relevant survivors are discussed above; both separately required aggregation mutations were independently exercised (`gate-logs/C5-mutants.log:13`, `gate-logs/C5-mutants.log:27`). |
| T4-batch-review | FAIL | Its nine recorded entries repeat four distinct classes: absent-code fabrication, unreadable-response classification, missing failure-boundary tests and detached fixture tasks. These were independently grounded above; repeated entries are not nine independent defects (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | `pr-description.md` is absent by design at Check; the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No human clearance is owed for this deferral. |
| host-tikv | PASS from frozen log | The log records successful clippy compilation for both the TiKV crate and server feature selection; this establishes compilation, not a live TiKV integration exercise (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`). |

Prior art was checked by actual affected file path, rather than only by PR title. GitHub merged history was queried for `crates/validate`, both Cargo files, both deny files and the architecture document; all 356 returned closed/merged PRs were also filtered by their changed-file paths. No prior PR touched any affected `crates/validate` file. Related history includes waiver PR #727 and SDK bumps #796/#836; the target already locks SDK 1.148.0 and lru 0.18.4, explaining why the old waiver's removal trigger is satisfied (`reviewer-merged-history.log:1`, `reviewer-prior-art.log:4`, `Cargo.lock:210`, `Cargo.lock:2139`). The disposable target has only a synthetic base commit, so its local log alone could not establish this history.

The binding client criterion exercised the real SDK and real Wyrd gateway over TCP with redb, memory coordination and filesystem storage. It did not substitute a shim for Rust or a canned server for the binding round-trip/streaming tests. The additional raw-response probes isolate error handling; they do not replace that evidence. No undischarged external dependency was found for the bounded criterion in `brief.md:178`. The HTTP/TLS and 5 GiB limits are already scoped and tracked, and the accepted dependency decision is carried forward for the brief's required sign-off confirmation. The capability-probe smell test found no load-time capability guard papering over an eager cause.

The independent adversarial probes remain reproducible from this review directory with:

```sh
CARGO_TARGET_DIR="$PWD/pdca-reviewer-741-scratch/build" \
TMPDIR="$PWD/pdca-reviewer-741-scratch" \
cargo test --offline \
  --manifest-path pdca-reviewer-741-scratch/pdca-reviewer-741-probes/Cargo.toml \
  -- --nocapture --test-threads=1
```

Expected result on this patch: eight pass, and the empty-404 and unreadable-response assertions fail. This report is advisory; it does not accept or reject the bundle on the harness's behalf.

### Advisory — adversary

# Adversarial review — issue #741 (validate-s3-client-layer)

Method: copied `$PDCA_TARGET` (patch applied) into a scratch dir, ran
`cargo test -p wyrd-validate --test s3_client_roundtrip` (4/4 pass), then ran mutations
against `crates/validate/src/client.rs` and probe tests that point the production
`S3Client` at hand-written loopback responses and at the real in-process gateway.
Line numbers below are on the target checkout.

## What I could not break

- **Streaming oracles hold.** Tried to refute criterion 3 by mutation; could not.
  Buffering the PUT body before `send()` (collect the stream at `crates/validate/src/client.rs:158`)
  fails the PUT leg with "relay had forwarded only 0 request bytes" (`crates/validate/tests/s3_client_roundtrip.rs:368`).
  Collecting the GET body inside `get_object` (`client.rs:206`) fails the GET leg with
  "no GET response within 15s" (`s3_client_roundtrip.rs:416`). A lazy "collect on the first
  `next_chunk`, then hand out 64 KiB pieces" variant (`client.rs:321`) also fails, with
  "no body piece within 15s" (`s3_client_roundtrip.rs:428`). Both legs run the production
  path through a real relay. One weak spot: the `held <= GET_PREFIX` assertion at
  `s3_client_roundtrip.rs:438` can never fail, because the relay cannot forward past the limit
  before release. The 15 s bound is what actually catches a collecting client.
- **PUT body length checks hold.** Against the real gateway, a source that overruns
  (declared 5, yields 10) and one that runs short (declared 10, yields 5) both return
  `S3Error::Body`, and a later GET returns `NoSuchKey`, so nothing was stored. A GET body cut
  short of its declared `Content-Length` returns `S3Error::Body`. The guards at
  `client.rs:327`, `:338` and `:407` are never the thing that fires, though: hyper or the SDK's
  chunked-upload layer catches it first. That is why C5 lists them as missed mutants. T4
  already has this test gap, so I am not raising it again.
- **Gate shape.** C4-verify `UNVERIFIABLE` is the outcome the brief predicted. The log shows
  the reverted base failing to compile because the symbols this patch adds are missing. C4-diff-cov
  "does not apply on origin/main" means the patch was built on the merged wave-0 base, not
  `origin/main`. Neither counts as evidence against the fix. The gate fails overall because of
  T4-batch-review's 9 blocking findings. C4-ci being green proves little about the error mapping:
  its tests never reach any of the cases below.

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:249-252`: an empty-body 404 is reported as
  a `<Code>` the server never sent. Reproduced: a loopback server answering
  `404, Content-Length: 0, x-amz-request-id: …` gives
  `Service{status: 404, code: Code("NotFound"), message: None}` for both GET and DELETE.
  The SDK makes this code up for any empty 404 (`aws-sdk-s3-1.148.0/src/protocol_serde.rs:24-28`),
  and `describe` trusts `meta.code()` before it checks `has_body`. That breaks the type's own
  rule (`crates/validate/src/error.rs:61`, "The body carried this `<Code>`") and the brief's
  Open Question 4 (keep "no body" apart). The result should be `ErrorCode::NoBody`. This
  confirms T4's finding with a reproduction.
- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:241-262`: a success response the SDK
  cannot parse comes back as an S3 *error response*, and the only diagnostic is thrown away.
  Reproduced: `200 OK, Content-Length: 5, Last-Modified: not-a-date`, body `hello`, gives
  `Service{status: 200, code: NoBody, message: None}`. That is three wrong facts. It is a
  success status filed as an error response. It says "no body" when 5 bytes arrived (the SDK's
  deserializer had already swapped the body out, so `bytes()` returns `None`). And it drops the
  SDK's "Failed to parse LastModified from header" (`aws-sdk-s3-1.148.0/src/protocol_serde/shape_get_object.rs:136`),
  because `describe` reads only `meta()`. `error.rs:23-24` names this exact case ("a malformed
  header") as `Unreadable`, and `docs/design/architecture/05-building-block-view.md:253` claims
  the two are kept apart. This is the defect a validator most needs to name: a gateway emitting
  a timestamp that fails its grammar is a class the rubric lists. The same arm turns an HTML 500
  into `Service{500, NoCodeInBody, request_id: None}` (T4's finding). Fix: route the operation's
  `Unhandled` service errors, and any 2xx, to `Unreadable`, and keep `DisplayErrorContext` as `detail`.
- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:326-333`: a torn GET body with no declared
  length is silently accepted as a complete, shorter object. Reproduced: `200 OK` with no
  `Content-Length`, no chunked encoding, `Connection: close`, body `hel`, then close, gives
  `content_length() == None`, then `"hel"`, then `Ok(None)`. No error at any point. The rubric's
  protocol-input rule says torn or truncated input must be an error or indeterminate, never
  silently accepted. The doc at `client.rs:317-319` promises "never a shorter object" but only
  checks it when a length was declared. S3 GetObject always declares `Content-Length`, so a body
  framed only by connection close should be refused, or at least flagged so the layer above
  cannot mistake it for a clean read.
- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_roundtrip.rs:328-336` (and `:252`):
  criterion 2 asks for "the `x-amz-request-id` the gateway stamps", but the tests only check the
  format (32 lowercase hex characters), and the PUT receipt only checks `is_some()`. Mutation:
  replace `client.rs:258-261` with `request_id: Some("0123456789abcdef0123456789abcdef".to_string())`.
  All 4 tests still pass. So the "asserted field by field" claim is not backed for this field.
  Fix: the relay already sees the response bytes, so have it capture the `x-amz-request-id`
  header and assert the typed error's id equals it.
- NEEDS-HUMAN [human] — `deny.toml` / `deny-all-features.toml` (the old RUSTSEC-2026-0253 entry,
  base `deny.toml:86`): the brief said to keep the waiver, rewrite its rationale, and record that
  the maintainer accepted an unsound `lru` in a shipped binary. The patch deletes the waiver
  instead. Checked: the deletion is correct. The base lockfile already resolves `aws-sdk-s3`
  1.148.0 (`Cargo.lock:210-211`) and `lru` 0.18.4 (`Cargo.lock:2139-2140`). `aws-sdk-s3` 1.144.0+
  requires `lru ^0.18.2`, while 1.142.0 still required `^0.16.3`. So the advisory matched nothing
  before this patch, and the waiver's own removal trigger had already fired. (I could not check
  1.143.0, so the "first release" claim at root `Cargo.toml:94` is unverified. The 1.144.0 floor
  is safe either way.) What a human must decide: the brief's header asks sign-off §9 to confirm
  and post to #741 that the RUSTSEC-2026-0253 exposure was *accepted*. That sentence would now be
  false. No unsound `lru` ships. The tracker note should say the exposure is moot, and the
  departure from the brief should be recorded as deliberate.
- NEEDS-HUMAN [impl] — root `Cargo.toml:92` (audit prose for the shipped dependency): it says
  "no rustls", but the `default-client` feature chosen at `Cargo.toml:100` pulls
  `rustls-native-certs`, `rustls-pki-types` and, on Unix, `openssl-probe` into the shipped graph
  (`cargo tree -p wyrd-validate -e normal`). The `rustls` crate itself is not pulled in, and
  `cargo deny` passes, so this is only an accuracy fix to the transitive-surface line of the
  audit the brief asked for. Name these crates.

### Advisory — code-review

Review based on the diff, target source, and frozen gate evidence; no gates were rerun.

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:241`: An SDK `ServiceError` does not guarantee successfully parsed S3 XML. As recorded in the frozen T4 evidence, the SDK also wraps XML/header parsing failures in operation-level unhandled service errors. This unconditional conversion turns malformed error responses into `Service(NoCodeInBody)` and discards the parsing diagnostic. Preserve these as `Unreadable` with status, request ID, and cause; add a raw-response regression distinguishing malformed XML from valid XML without `<Code>`.

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:249`: Checking `meta.code()` before body presence invents a received S3 code for an empty HTTP 404: the SDK synthesizes `NotFound` in that case (also identified by the frozen T4 evidence). The result becomes `Code("NotFound")` rather than `NoBody`, breaking the distinction the public error type promises. Check body absence before accepting SDK metadata and cover an empty 404 with field-by-field assertions.

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:321` and `crates/validate/src/client.rs:407`: The four new integration tests never exercise deadline expiry, truncated GET responses, or short, oversized, and failing PUT sources. The frozen mutation run confirms that deleting the short-body checks survives. The GET test's outer timeout tests streaming progress, not the client's timeout classification. Add focused failure-path tests asserting `Timeout` phase and `Body` variants, including a PUT whose excess arrives in a separate chunk, so these new guarantees cannot silently regress.

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_roundtrip.rs:175`: The relay discards its listener and both pump task handles; the gateway does the same at `crates/validate/tests/s3_client_roundtrip.rs:102`. Dropping the fixture leaves helpers and sockets alive until runtime teardown, and the gateway can outlive its temporary storage. This violates the standing abort-on-drop convention. Own the tasks in fixture guards, including child pumps, and abort them when the fixture drops.

No additional material reuse, simplification, or efficiency findings.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Correct and regression-test the SDK-to-wire error interpretation — treating all SDK service metadata as received S3 facts fabricates an absent code and loses unreadable-response diagnostics (`crates/validate/src/client.rs:241`, `crates/validate/src/client.rs:249`, `reviewer-probes.log:21`, `reviewer-probes.log:27`).
- [ ] T5 Judgment — Add shipped deadline-expiry, truncated-GET and failing/length-mismatched PUT regressions — the new bounded-wait and integrity guarantees currently lack those tests, and relevant guard-removal mutants survive (`crates/validate/src/client.rs:224`, `crates/validate/src/client.rs:320`, `crates/validate/src/client.rs:393`, `gate-logs/C5-mutants.log:18`, `gate-logs/C5-mutants.log:20`).
- [ ] Validation — fitness-to-purpose — Confirm this HTTP-only client-layer milestone, its declared criterion-absence red, and the session-recorded shipped-SDK acceptance at sign-off — deployed TLS topology and the 5 GiB memory property remain outside this slice (`brief.md:17`, `brief.md:185`, `brief.md:196`, `brief.md:430`).
- [ ] C4-verify — The frozen log shows four green tests followed by compile-only pre-fix failure, exactly as independently reproduced; sign-off must accept the explicitly declared new-API proof posture (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:277`, `brief.md:185`).
- [ ] `crates/validate/src/client.rs:249-252`: an empty-body 404 is reported as a `<Code>` the server never sent. Reproduced: a loopback server answering `404, Content-Length: 0, x-amz-request-id: …` gives `Service{status: 404, code: Code("NotFound"), message: None}` for both GET and DELETE. The SDK makes this code up for any empty 404 (`aws-sdk-s3-1.148.0/src/protocol_serde.rs:24-28`), and `describe` trusts `meta.code()` before it checks `has_body`. That breaks the type's own rule (`crates/validate/src/error.rs:61`, "The body carried this `<Code>`") and the brief's Open Question 4 (keep "no body" apart). The result should be `ErrorCode::NoBody`. This confirms T4's finding with a reproduction.
- [ ] `crates/validate/src/client.rs:241-262`: a success response the SDK cannot parse comes back as an S3 *error response*, and the only diagnostic is thrown away. Reproduced: `200 OK, Content-Length: 5, Last-Modified: not-a-date`, body `hello`, gives `Service{status: 200, code: NoBody, message: None}`. That is three wrong facts. It is a success status filed as an error response. It says "no body" when 5 bytes arrived (the SDK's deserializer had already swapped the body out, so `bytes()` returns `None`). And it drops the SDK's "Failed to parse LastModified from header" (`aws-sdk-s3-1.148.0/src/protocol_serde/shape_get_object.rs:136`), because `describe` reads only `meta()`. `error.rs:23-24` names this exact case ("a malformed header") as `Unreadable`, and `docs/design/architecture/05-building-block-view.md:253` claims the two are kept apart. This is the defect a validator most needs to name: a gateway emitting a timestamp that fails its grammar is a class the rubric lists. The same arm turns an HTML 500 into `Service{500, NoCodeInBody, request_id: None}` (T4's finding). Fix: route the operation's `Unhandled` service errors, and any 2xx, to `Unreadable`, and keep `DisplayErrorContext` as `detail`.
- [ ] `crates/validate/src/client.rs:326-333`: a torn GET body with no declared length is silently accepted as a complete, shorter object. Reproduced: `200 OK` with no `Content-Length`, no chunked encoding, `Connection: close`, body `hel`, then close, gives `content_length() == None`, then `"hel"`, then `Ok(None)`. No error at any point. The rubric's protocol-input rule says torn or truncated input must be an error or indeterminate, never silently accepted. The doc at `client.rs:317-319` promises "never a shorter object" but only checks it when a length was declared. S3 GetObject always declares `Content-Length`, so a body framed only by connection close should be refused, or at least flagged so the layer above cannot mistake it for a clean read.
- [ ] `crates/validate/tests/s3_client_roundtrip.rs:328-336` (and `:252`): criterion 2 asks for "the `x-amz-request-id` the gateway stamps", but the tests only check the format (32 lowercase hex characters), and the PUT receipt only checks `is_some()`. Mutation: replace `client.rs:258-261` with `request_id: Some("0123456789abcdef0123456789abcdef".to_string())`. All 4 tests still pass. So the "asserted field by field" claim is not backed for this field. Fix: the relay already sees the response bytes, so have it capture the `x-amz-request-id` header and assert the typed error's id equals it.
- [ ] `deny.toml` / `deny-all-features.toml` (the old RUSTSEC-2026-0253 entry, base `deny.toml:86`): the brief said to keep the waiver, rewrite its rationale, and record that the maintainer accepted an unsound `lru` in a shipped binary. The patch deletes the waiver instead. Checked: the deletion is correct. The base lockfile already resolves `aws-sdk-s3` 1.148.0 (`Cargo.lock:210-211`) and `lru` 0.18.4 (`Cargo.lock:2139-2140`). `aws-sdk-s3` 1.144.0+ requires `lru ^0.18.2`, while 1.142.0 still required `^0.16.3`. So the advisory matched nothing before this patch, and the waiver's own removal trigger had already fired. (I could not check 1.143.0, so the "first release" claim at root `Cargo.toml:94` is unverified. The 1.144.0 floor is safe either way.) What a human must decide: the brief's header asks sign-off §9 to confirm and post to #741 that the RUSTSEC-2026-0253 exposure was *accepted*. That sentence would now be false. No unsound `lru` ships. The tracker note should say the exposure is moot, and the departure from the brief should be recorded as deliberate.
- [ ] root `Cargo.toml:92` (audit prose for the shipped dependency): it says "no rustls", but the `default-client` feature chosen at `Cargo.toml:100` pulls `rustls-native-certs`, `rustls-pki-types` and, on Unix, `openssl-probe` into the shipped graph (`cargo tree -p wyrd-validate -e normal`). The `rustls` crate itself is not pulled in, and `cargo deny` passes, so this is only an accuracy fix to the transitive-surface line of the audit the brief asked for. Name these crates.
- [ ] `crates/validate/src/client.rs:241`: An SDK `ServiceError` does not guarantee successfully parsed S3 XML. As recorded in the frozen T4 evidence, the SDK also wraps XML/header parsing failures in operation-level unhandled service errors. This unconditional conversion turns malformed error responses into `Service(NoCodeInBody)` and discards the parsing diagnostic. Preserve these as `Unreadable` with status, request ID, and cause; add a raw-response regression distinguishing malformed XML from valid XML without `<Code>`.
- [ ] `crates/validate/src/client.rs:249`: Checking `meta.code()` before body presence invents a received S3 code for an empty HTTP 404: the SDK synthesizes `NotFound` in that case (also identified by the frozen T4 evidence). The result becomes `Code("NotFound")` rather than `NoBody`, breaking the distinction the public error type promises. Check body absence before accepting SDK metadata and cover an empty 404 with field-by-field assertions.
- [ ] `crates/validate/src/client.rs:321` and `crates/validate/src/client.rs:407`: The four new integration tests never exercise deadline expiry, truncated GET responses, or short, oversized, and failing PUT sources. The frozen mutation run confirms that deleting the short-body checks survives. The GET test's outer timeout tests streaming progress, not the client's timeout classification. Add focused failure-path tests asserting `Timeout` phase and `Body` variants, including a PUT whose excess arrives in a separate chunk, so these new guarantees cannot silently regress.
- [ ] `crates/validate/tests/s3_client_roundtrip.rs:175`: The relay discards its listener and both pump task handles; the gateway does the same at `crates/validate/tests/s3_client_roundtrip.rs:102`. Dropping the fixture leaves helpers and sockets alive until runtime teardown, and the gateway can outlive its temporary storage. This violates the standing abort-on-drop convention. Own the tasks in fixture guards, including child pumps, and abort them when the fixture drops.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_741/review-b
- [ ] The brief assumes a base that does not exist yet. It says the crate “DOES exist pre-patch” and must build on “#740's accepted result” (`brief.md:49-52`, `brief.md:67-69`), but the declared prerequisite is only `PLANNED` (`dependency-state.json:2-5`), and the resolved `origin/main` target has no `crates/validate` member at all (`Cargo.toml:9-32`). On this target, `cargo test -p wyrd-validate ...` cannot run. Revise the target/ordering to require a materialized #740 result rather than treating planned work as the pre-patch tree.
- [ ] The brief suppresses the tracker’s load-bearing dependency decision without tracker evidence. The only recorded maintainer comment says the shipped `aws-sdk-s3` move “needs the audit **before** the crate lands” and “Confirm before starting” (`notes.json:1`); the brief instead declares the exposure “SETTLED,” says its audit is merely a slice deliverable, and orders Do not to reopen it (`brief.md:10-14`, `brief.md:243-270`). Add the missing recorded acceptance or retain the human dependency/license decision as unresolved.
- [ ] Criterion 3 does not yet falsify the claimed *bidirectional client* memory invariant. Its permitted observables are the largest buffer requested from the PUT source or live generated PUT bytes (`brief.md:35-39`, `brief.md:221-226`); reading GET output incrementally can still pass if the client first collects the whole response and then yields small chunks. The target source guarantees only that the **gateway** streams (`crates/gateway-s3/src/lib.rs:12-17`), not that this new client does. Require a concrete aggregation-sensitive oracle for PUT and GET, and require the deliberate-buffering mutation to fail on each direction.
- [ ] The scope contains an optional drive-by change that breaks its own parallel-work claim. It says this slice touches `crates/validate/**`, root `Cargo.toml`, and `deny.toml`, disjoint from #738’s `crates/server/**` (`brief.md:70-76`), but later recommends deduplicating the server’s SDK dependency even though that is “not required by the criterion” (`brief.md:323-325`). The target’s pins are inline in `crates/server/Cargo.toml:126-130`, so deduplication necessarily adds the very `crates/server/**` edit the brief said was excluded. Remove that cleanup or declare the overlap and scope explicitly.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Correct and regression-test the SDK-to-wire error interpretation — treating all SDK service metadata as received S3 facts fabricates an absent code and loses unreadable-response diagnostics (`crates/validate/src/client.rs:241`, `crates/validate/src/client.rs:249`, `reviewer-probes.log:21`, `reviewer-probes.log:27`).; T5 Judgment — Add shipped deadline-expiry, truncated-GET and failing/length-mismatched PUT regressions — the new bounded-wait and integrity guarantees currently lack those tests, and relevant guard-removal mutants survive (`crates/validate/src/client.rs:224`, `crates/validate/src/client.rs:320`, `crates/validate/src/client.rs:393`, `gate-logs/C5-mutants.log:18`, `gate-logs/C5-mutants.log:20`).; C4-verify — The frozen log shows four green tests followed by compile-only pre-fix failure, exactly as independently reproduced; sign-off must accept the explicitly declared new-API proof posture (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:277`, `brief.md:185`).; `crates/validate/src/client.rs:249-252`: an empty-body 404 is reported as a `<Code>` the server never sent. Reproduced: a loopback server answering `404, Content-Length: 0, x-amz-request-id: …` gives `Service{status: 404, code: Code("NotFound"), message: None}` for both GET and DELETE. The SDK makes this code up for any empty 404 (`aws-sdk-s3-1.148.0/src/protocol_serde.rs:24-28`), and `describe` trusts `meta.code()` before it checks `has_body`. That breaks the type's own rule (`crates/validate/src/error.rs:61`, "The body carried this `<Code>`") and the brief's Open Question 4 (keep "no body" apart). The result should be `ErrorCode::NoBody`. This confirms T4's finding with a reproduction.; `crates/validate/src/client.rs:241-262`: a success response the SDK cannot parse comes back as an S3 *error response*, and the only diagnostic is thrown away. Reproduced: `200 OK, Content-Length: 5, Last-Modified: not-a-date`, body `hello`, gives `Service{status: 200, code: NoBody, message: None}`. That is three wrong facts. It is a success status filed as an error response. It says "no body" when 5 bytes arrived (the SDK's deserializer had already swapped the body out, so `bytes()` returns `None`). And it drops the SDK's "Failed to parse LastModified from header" (`aws-sdk-s3-1.148.0/src/protocol_serde/shape_get_object.rs:136`), because `describe` reads only `meta()`. `error.rs:23-24` names this exact case ("a malformed header") as `Unreadable`, and `docs/design/architecture/05-building-block-view.md:253` claims the two are kept apart. This is the defect a validator most needs to name: a gateway emitting a timestamp that fails its grammar is a class the rubric lists. The same arm turns an HTML 500 into `Service{500, NoCodeInBody, request_id: None}` (T4's finding). Fix: route the operation's `Unhandled` service errors, and any 2xx, to `Unreadable`, and keep `DisplayErrorContext` as `detail`.; `crates/validate/src/client.rs:326-333`: a torn GET body with no declared length is silently accepted as a complete, shorter object. Reproduced: `200 OK` with no `Content-Length`, no chunked encoding, `Connection: close`, body `hel`, then close, gives `content_length() == None`, then `"hel"`, then `Ok(None)`. No error at any point. The rubric's protocol-input rule says torn or truncated input must be an error or indeterminate, never silently accepted. The doc at `client.rs:317-319` promises "never a shorter object" but only checks it when a length was declared. S3 GetObject always declares `Content-Length`, so a body framed only by connection close should be refused, or at least flagged so the layer above cannot mistake it for a clean read.; `crates/validate/tests/s3_client_roundtrip.rs:328-336` (and `:252`): criterion 2 asks for "the `x-amz-request-id` the gateway stamps", but the tests only check the format (32 lowercase hex characters), and the PUT receipt only checks `is_some()`. Mutation: replace `client.rs:258-261` with `request_id: Some("0123456789abcdef0123456789abcdef".to_string())`. All 4 tests still pass. So the "asserted field by field" claim is not backed for this field. Fix: the relay already sees the response bytes, so have it capture the `x-amz-request-id` header and assert the typed error's id equals it.; root `Cargo.toml:92` (audit prose for the shipped dependency): it says "no rustls", but the `default-client` feature chosen at `Cargo.toml:100` pulls `rustls-native-certs`, `rustls-pki-types` and, on Unix, `openssl-probe` into the shipped graph (`cargo tree -p wyrd-validate -e normal`). The `rustls` crate itself is not pulled in, and `cargo deny` passes, so this is only an accuracy fix to the transitive-surface line of the audit the brief asked for. Name these crates.; `crates/validate/src/client.rs:241`: An SDK `ServiceError` does not guarantee successfully parsed S3 XML. As recorded in the frozen T4 evidence, the SDK also wraps XML/header parsing failures in operation-level unhandled service errors. This unconditional conversion turns malformed error responses into `Service(NoCodeInBody)` and discards the parsing diagnostic. Preserve these as `Unreadable` with status, request ID, and cause; add a raw-response regression distinguishing malformed XML from valid XML without `<Code>`.; `crates/validate/src/client.rs:249`: Checking `meta.code()` before body presence invents a received S3 code for an empty HTTP 404: the SDK synthesizes `NotFound` in that case (also identified by the frozen T4 evidence). The result becomes `Code("NotFound")` rather than `NoBody`, breaking the distinction the public error type promises. Check body absence before accepting SDK metadata and cover an empty 404 with field-by-field assertions.; `crates/validate/src/client.rs:321` and `crates/validate/src/client.rs:407`: The four new integration tests never exercise deadline expiry, truncated GET responses, or short, oversized, and failing PUT sources. The frozen mutation run confirms that deleting the short-body checks survives. The GET test's outer timeout tests streaming progress, not the client's timeout classification. Add focused failure-path tests asserting `Timeout` phase and `Body` variants, including a PUT whose excess arrives in a separate chunk, so these new guarantees cannot silently regress.; `crates/validate/tests/s3_client_roundtrip.rs:175`: The relay discards its listener and both pump task handles; the gateway does the same at `crates/validate/tests/s3_client_roundtrip.rs:102`. Dropping the fixture leaves helpers and sockets alive until runtime teardown, and the gateway can outlive its temporary storage. This violates the standing abort-on-drop convention. Own the tasks in fixture guards, including child pumps, and abort them when the fixture drops.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_741/review-b. 6 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
