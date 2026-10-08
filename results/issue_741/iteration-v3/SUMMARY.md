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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 75 mutants tested in 2m: 19 caught, 56 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_741/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.58s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Advisory FAIL for #741's S3 client foundation: round trips and streaming work, but three independently reproduced response-handling and resource-lifetime defects remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The foundation has falsifiable wire-level criteria and explicit HTTP-only, in-process scope; the 5 GiB property remains settled in #761 (`brief.md:38`, `brief.md:161`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the patch while retaining the regression test reproduces the declared missing-API compile failure, not a behavioral assertion failure (`review-red.log:6`; `brief.md:185`). |
| C3 Change | PASS | The changes stay within the client/dependency/doc surfaces and preserve the prerequisite crate; no server implementation changes appear (`crates/validate/Cargo.toml:23`; `docs/design/architecture/05-building-block-view.md:253`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared new-API proof posture — pre-fix evidence is criterion absence; restored production passes 19 tests and both buffering mutations fail, but this does not discharge the three counterexamples below (`review-restored-green.log:26`, `review-mutation-put.log:9`, `review-mutation-get.log:9`, `review-probes-final.log:37`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Complete error-body bounds, framing validation, and upload cancellation with regressions — F1–F3 survive the existing 19-test suite and can make a validator exhaust resources or trust malformed responses (`crates/validate/src/client.rs:358`, `crates/validate/src/client.rs:496`; `review-probes-final.log:7`). |
| T1 Structure | PASS | The normal dependency closure remains independent of Wyrd implementations; concrete backends appear only in the real gateway test fixture (`crates/validate/Cargo.toml:23`, `crates/validate/tests/s3_client_roundtrip.rs:136`; `review-ci.log:18`). |
| T2 Shape | PASS | The public client/error split and architecture update fit this foundation slice; runtime deadlines share Tokio's clock and no capability/load-time probe masks the cause (`crates/validate/src/lib.rs:23`, `crates/validate/src/client.rs:13`; `docs/design/architecture/05-building-block-view.md:253`). |
| T3 Runtime | FAIL | Faulty endpoints can cause unbounded error aggregation, retain a blocked upload after its deadline, and have conflicting error lengths accepted; all three have executable counterexamples (F1–F3; `crates/validate/src/client.rs:358`, `crates/validate/src/client.rs:496`). |
| T4 Contribution | FAIL | The frozen batch's eight reports reduce to three unresolved, reproduced defects, so its findings-disposition obligation remains open; the publish-only contribution-artifact audit is separately N/A (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`; `AGENTS.md:206`). |
| T5 Judgment | NEEDS-HUMAN | Confirm the recorded SDK-adoption acceptance and required ADR-0003 audit at sign-off — this is a new shipped dependency and the brief locates acceptance in the Plan session; do not reopen the settled dependency choice (`brief.md:17`, `brief.md:347`; `Cargo.toml:84`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the HTTP-only, redb/mem/local-FS wire proof is sufficient for this foundation once F1–F3 are addressed — it does not establish TLS, production topology, or the separately scoped 5 GiB property (`brief.md:196`, `brief.md:430`). |

Source citations refer to the patched disposable `$PDCA_TARGET` (`target/`); brief and evidence citations refer to this review directory. No finding relies on another checkout or on withheld build notes.

1. **F1 — FAIL: bound error bodies before SDK aggregation (P1).** `crates/validate/src/client.rs:123` installs the stock connector, and `:358` reads an already-collected error body; `:282` supplies a time limit but no byte limit. A loopback endpoint generated 64 MiB of XML padding from a 64 KiB buffer, and DELETE returned ordinary `Service(503, SlowDown)` after receiving all of it (`review-probes-final.log:23`). This demonstrates size-proportional buffering; it does not claim an actual OOM was induced. A larger or continuously streamed error can exhaust memory before the default 300-second deadline. Enforce a cumulative error-body budget before collection/deserialization, and return an explicit unreadable/oversize failure when it is exceeded. This is the rubric's oversize-input boundary (`AGENTS.md:161`).

2. **F2 — FAIL: cancel uploads independently of body polling (P1).** `crates/validate/src/client.rs:496` only sets a flag and wakes the body consumer; the stop check at `:531` requires another body poll. With a 512 MiB generated upload using 32 MiB frames, a peer stopped reading, allowed socket backpressure to build, then sent an early 200. PUT correctly returned `Body`, but its source remained retained for four more seconds despite a three-second operation limit. It was released only when the peer closed (`review-probes-final.log:7`). The small-frame probe released successfully; the existing tiny pending-source and draining-peer tests therefore do not cover this case (`crates/validate/tests/s3_client_roundtrip.rs:1132`, `:1169`). Cancellation must release the source/connection even when socket writes cannot progress, as required by `AGENTS.md:181`. The limitation documented at `client.rs:171` has no tracked-issue deferral and does not satisfy that rule.

3. **F3 — FAIL: enforce error-response framing for PUT and DELETE (P2).** `crates/validate/src/client.rs:358` validates the collected XML without checking it against the declared HTTP length before returning `Service` at `:373`. The same complete, chunk-framed 60-byte `<Error>` response with `Content-Length: 1000` produces `Unreadable` for GET but `Service(503, SlowDown)` for PUT and DELETE (`review-probes-final.log:16`). Thus the SDK's GET length enforcement does not protect the other operations. Reject conflicting framing or explicitly enforce declared length on every error response; extend the current GET-only conflicting-length regression (`crates/validate/tests/s3_client_roundtrip.rs:920`). Otherwise later matrix decisions can treat malformed responses as authoritative S3 refusals.

The three probes are preserved in `pdca-reviewer-741-scratch/reviewer_741_probes.rs`. Run `python3 pdca-reviewer-741-scratch/reproduce-probes.py` from this leaf to reproduce all three failures against the disposable target; the runner restores its test tree afterward. Production source was not fixed by this review.

The binding streaming evidence is strong within its stated scope. Independently buffering PUT made the existing oracle observe zero forwarded bytes when the generator finished (`review-mutation-put.log:9`). Independently collecting GET made its existing 15-second withheld-tail assertion fail (`review-mutation-get.log:9`). Both mutations were reverted, and all 19 original integration tests then passed (`review-restored-green.log:26`). The new failure-path probes exercise the actual client over TCP, without aliases, mocked transports, or external services.

| Frozen gate | Adjudication | Evidence and limits |
|-------------|--------------|---------------------|
| C4-ci | PASS in frozen run; local host caveat | Frozen output shows all checks passed, including dependency walls and DST (`gate-logs/C4-ci.log:3307`, `:3908`). Independent `cargo xtask ci` passed prose/docs, guards, fmt, workspace clippy/build/tests and machete, then cargo-deny failed to acquire its advisory DB lock on a read-only home path (`review-ci.log:3104`). An offline retry failed identically (`review-deny.log:1`). This is not a patch defect or a fully green local CI run. Independent conformance and statics also passed (`review-conformance.log:1`, `review-statics.log:3`). |
| C4-verify | NEEDS-HUMAN, declared proof posture | Frozen green is 19 tests; red is missing imports, exactly as independently reproduced (`gate-logs/C4-verify.log:10`, `:16`; `review-red.log:6`). The C4 table row states the sign-off decision. |
| C4-diff-cov | N/A to patch correctness; measurement unavailable | The wrapper could not apply the stacked patch to `origin/main` (`gate-logs/C4-diff-cov.log:10`). The supplied target has the prerequisite crate and builds successfully. No coverage percentage was measured; the old-base failure is not a source/compile defect. |
| C5-mutants | PASS, limited evidence | The frozen log reports 19 caught and 56 unviable mutants, with none reported surviving (`gate-logs/C5-mutants.log:13`). This is not 75 behaviorally caught mutants; it does not test F1–F3. The two required aggregation mutations were independently exercised above. |
| T4-batch-review | FAIL, grounded | Its eight reports contain three distinct defects, all independently reproduced as F1–F3 (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | `pr-description.md` is absent by design at Check; the substantive artifact audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No human clearance is requested for this deferred row. |
| host-tikv | PASS in frozen evidence | The captured output shows both requested feature-enabled clippy compilations finishing successfully (`gate-logs/host-tikv.log:110`, `:209`); this review did not independently repeat those unrelated feature builds. |

The affected-path prior-art check completed. The target has only its synthetic base commit, so read-only GitHub history and closed-PR file lists supplied the history evidence: 356 closed PRs were enumerated, 124 intersected the ten patch paths, and none touched `crates/validate/`. Closed-unmerged overlaps were unrelated dependency bumps. Relevant merged work is the earlier waiver (#727) and SDK lockfile updates (#796, #836); the patch's lockfile change adds only validator dependency edges. Commands/results and raw records are preserved in `review-prior-art.log:1` and the JSON files it names. No competing client implementation or rejected client design was found.

The waiver removal is consistent with the supplied tree: the manifest now requires SDK >=1.144.0 (`Cargo.toml:98`), while the lockfile already resolves SDK 1.148.0 and lru 0.18.4 (`Cargo.lock:210`, `:2139`). The frozen dependency audit passes without the obsolete waiver. The previous absent-code, malformed-XML, missing-length, and early-success findings have concrete tests and pass; they are not re-raised here. TLS and the 5 GiB property remain scoped elsewhere.

All ten original patch files match their pre-review hashes, and the changed/untracked source-file set is unchanged (`review-restoration.log:1`). Harness-owned evidence remains in this directory for disposal by the harness.

### Advisory — adversary

# Adversarial review — issue #741 (validate-s3-client-layer), round 3

How I checked: I copied `$PDCA_TARGET` (patch applied) to scratch and built it (cargo 1.96). The
19-test suite passed 3 of 3 runs. I then wrote throwaway probe tests against the production
`S3Client`, using the suite's own scripted-endpoint helpers. Every result below is from a run,
not from reading code.

## Refutations that landed

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:358-361`: on PUT and DELETE, an error body
  shorter than its declared `Content-Length` is reported as a clean S3 error. That breaks the
  type's own promise at `crates/validate/src/error.rs:29` ("an error body shorter than its
  declared length" is `Unreadable`). Repro: a scripted `503` with `Transfer-Encoding: chunked` and
  `Content-Length: 500`, whose chunked body is a complete 56-byte
  `<Error><Code>SlowDown</Code><Message>m</Message></Error>`.
  DELETE gives `Service{503, Code("SlowDown"), message: Some("m")}`, and PUT gives the same.
  GET on the same bytes gives `Unreadable{… ContentLengthError { expected: 500, received: 56 }}`,
  because the SDK checks length only on GetObject. So one response gets two different
  classifications depending on the operation. This falls under the rubric's protocol-input class
  ("enforce declared `Content-Length`"). T4 already raised it; this confirms it with a
  reproduction. Fix: in `service_error`, compare the collected body length with any declared
  `Content-Length` (or refuse a response that carries both TE and CL) before trusting the SDK's
  reading. Add DELETE and PUT cases next to `an_error_body_cut_short_is_unreadable`
  (`crates/validate/tests/s3_client_roundtrip.rs:872`), which covers only the length-framed
  shape.

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:298` / `:127`: error bodies are read into
  memory with no byte cap. The only bound is the operation deadline, which defaults to 300 s
  (`client.rs:79`). Repro: a scripted endpoint answers DELETE with a chunked `500` and then streams
  1 MiB chunks, stopping at 768 MiB. With `operation_timeout = 20s` the call ends as
  `Timeout{Operation}`, and the process's peak memory (VmHWM) went from **19 MiB to 794 MiB**: the
  SDK held all 768 MiB before `service_error` ever ran. At the default deadline on a fast link
  that is many GiB. A broken or hostile endpoint can take the validator down with memory
  exhaustion, which is the failure the brief's "never holds … whole" reasoning exists to prevent.
  It also falls under the rubric's protocol-input class ("oversize input is … an error"). T4
  flagged it; this measures it. Possible fix: an SDK interceptor (`modify_before_deserialization`)
  that wraps non-2xx response bodies in a size-limited body, so going over the cap surfaces as
  `Unreadable`. Add a regression that streams past the cap.

## T4 findings that did NOT reproduce (the human adjudicating the gating T4 row should know)

- NEEDS-HUMAN [human] — T4's three blocking entries at `crates/validate/src/client.rs:496` claim
  that once hyper is stuck writing to a peer that stopped reading, the upload source and
  connection are held forever after the PUT returns. I could not reproduce this on the locked
  stack (hyper 1.10.1, aws-smithy-http-client 1.4.2), in three setups:
  1. Server reads only the head, never answers. Operation timeout of 1 s, 64 MiB generator: the
     source was released at once.
  2. Server reads the head, waits 1.5 s so the client fills the socket buffers (about 2.7 MiB
     sent), then sends `200` and never reads: the PUT returns `Body{request_id: Some(..)}` and
     the source is released within the 10 s window.
  3. Same as 2, but the server drains afterwards: it read about 3 MiB and then saw the client
     **close** the socket.

  So in my runs neither the source nor the socket outlived the PUT. That makes the patch's own
  caveat at `client.rs:170-171` ("a connection stuck writing … holds it until that server
  closes") look too pessimistic, and the T4 rows look like false positives on this dependency
  set. Whether to reject them, and whether to soften that doc line, is a sign-off call.

## Brief versus patch: a difference the reviewer could miss in either direction

- NEEDS-HUMAN [human] — base `deny.toml:77-86` and `deny-all-features.toml` (the
  RUSTSEC-2026-0253 waiver): the brief told Do to *rewrite* the waiver's rationale, keep its
  REMOVAL TRIGGER, and record the maintainer's acceptance of the exposure. The patch instead
  **deletes** the waiver from both files and pins `aws-sdk-s3 = "1.144.0"` (`Cargo.toml:94-98`),
  not the brief's `1.137.0`. This is correct on the facts. The **base** lockfile already resolves
  `aws-sdk-s3 1.148.0` → `lru 0.18.4`, and in the registry `aws-sdk-s3 1.142.0` depends on
  `lru ^0.16.3` while `1.144.0` depends on `^0.18.2`. So the waiver was already stale before this
  patch; its trigger had fired. `cargo deny check` is green (`gate-logs/C4-ci.log:3307-3311`).
  What follows from that: the RUSTSEC exposure the brief has sign-off §9 "confirm" no longer
  exists. The sentence mirrored onto #741 should say the advisory is not in the graph, not that
  an exposure was accepted. Deliverable (e) should be scored as superseded, not missing.

- NEEDS-HUMAN [human] — `crates/validate/src/client.rs:349` (and the disclosure at `:33`):
  request ids are read with the SDK's `RequestId` reader, which prefers `x-amzn-requestid` when it
  is present. Repro: a 404 carrying both `x-amz-request-id: scripted-request-741` and
  `x-amzn-requestid: from-a-proxy` gives `request_id: Some("from-a-proxy")`, and the id the
  gateway stamped is lost. Criterion 2 names "the `x-amz-request-id` the gateway stamps", and
  #529's join depends on that id. The Wyrd gateway never sends the `x-amzn` header, so this only
  matters if something in front of it adds one. The choice is disclosed; reading
  `raw.headers().get(REQUEST_ID_HEADER)` directly would remove the gap. Low priority.

## Tried to refute, could not

- **Criterion 3 evidence (the build notes were withheld from me, so I re-ran the mutations
  myself).** First I collected the PUT source in `put_object` before sending:
  `a_put_body_is_on_the_wire_before_the_generator_finishes` fails with "the relay had forwarded
  only 0 request bytes … held the 33554432-byte body". Then I collected `output.body` in
  `get_object`: `a_get_body_yields_while_the_response_tail_is_withheld` fails with "no GET
  response within 15s … collecting the body", and the round-trip test still passes, as it should.
  Both oracles go red on the real production path, and only for the reason they claim to test.
- **Criteria 1 and 2:** the request id is compared with the header the relay saw on the wire, so
  the iteration-1 mutation (a hard-coded id) can no longer pass. NoSuchKey after DELETE is
  asserted field by field.
- **The PUT outcome table (`client.rs:172-228`):** early ack while the tail is pending, a
  failure after the ack, refusal before the body, a timeout, and an empty object are all
  classified as documented. A 307 with no body gives `NoBody`; a BOM before `<Error>` is
  accepted; a whitespace-only body gives `Unreadable`.
- **Empty frames from the caller's source:** the SDK's aws-chunked encoder re-buffers into
  64 KiB chunks (`aws-runtime-1.10.0 …/http_body_1_x.rs:55-100`), so an empty piece cannot emit
  an early `0\r\n` terminator.
- **https endpoint downgraded to plaintext:** this cannot happen. `build_http()` uses hyper's
  `HttpConnector`, which refuses non-http schemes by default, and proxy env vars are not read
  (`ProxyConfig::disabled` unless configured).
- **Inherent limit, not a defect:** a server that reads only the head and answers `200` gets a
  **Receipt** if the source finished producing first. Repro: a 10-byte source gives
  `Ok(Receipt{… "early"})`. The client cannot see whether the peer read bytes it already wrote,
  and the patch's contract is worded as "produced", so this is consistent. The early-ack guard
  only catches servers that answer faster than the source produces.
- **C4-verify `UNVERIFIABLE`** is the compile-only red leg the brief declared in advance
  (`gate-logs/C4-verify.log`). It is not a defect.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:496`: Upload cancellation only sets a flag and wakes the body poller. If an endpoint answers early and then stops reading while Hyper is blocked flushing a large upload, Hyper cannot reach the next body poll, so the connection task, socket and caller's source remain retained after `put_object` returns. The large-source regression uses `Then::Drain` (`crates/validate/tests/s3_client_roundtrip.rs:1169`), which removes this backpressure. Make cancellation release the source and terminate the connection independently of write readiness; test a large upload against a peer that keeps the socket open without reading.

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:358`: Error-body validation runs after the SDK has collected the response without a byte limit. A faulty endpoint streaming a large non-success response can exhaust the validator's memory before the operation deadline expires; XML validation cannot prevent that allocation. Enforce a cumulative error-response byte budget before SDK aggregation, including chunked responses without a declared length, and add an oversized-response regression. The frozen T4 review corroborates this SDK behavior.

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:361`: A well-formed error document is accepted without checking its collected length against the response's `Content-Length`. For PUT/DELETE, a response with chunked framing, a conflicting declared length and complete `<Error>` XML becomes `Service` despite the malformed framing; the SDK length enforcement relied on elsewhere covers GET only. Reject conflicting framing or enforce the declared length before trusting the error fields, and add PUT/DELETE regressions. The existing mismatch test exercises only successful GET bodies (`crates/validate/tests/s3_client_roundtrip.rs:920`); the frozen T4 review identifies the uncovered error-response cases.

No additional reuse or simplification findings. Reviewed target source and frozen gate evidence; no tests rerun or target files modified. Frozen CI reports all 19 client integration tests passing.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared new-API proof posture — pre-fix evidence is criterion absence; restored production passes 19 tests and both buffering mutations fail, but this does not discharge the three counterexamples below (`review-restored-green.log:26`, `review-mutation-put.log:9`, `review-mutation-get.log:9`, `review-probes-final.log:37`).
- [ ] C5 Causal adequacy — Complete error-body bounds, framing validation, and upload cancellation with regressions — F1–F3 survive the existing 19-test suite and can make a validator exhaust resources or trust malformed responses (`crates/validate/src/client.rs:358`, `crates/validate/src/client.rs:496`; `review-probes-final.log:7`).
- [ ] T5 Judgment — Confirm the recorded SDK-adoption acceptance and required ADR-0003 audit at sign-off — this is a new shipped dependency and the brief locates acceptance in the Plan session; do not reopen the settled dependency choice (`brief.md:17`, `brief.md:347`; `Cargo.toml:84`).
- [ ] Validation — fitness-to-purpose — Decide whether the HTTP-only, redb/mem/local-FS wire proof is sufficient for this foundation once F1–F3 are addressed — it does not establish TLS, production topology, or the separately scoped 5 GiB property (`brief.md:196`, `brief.md:430`).
- [ ] C4-verify — Frozen green is 19 tests; red is missing imports, exactly as independently reproduced (`gate-logs/C4-verify.log:10`, `:16`; `review-red.log:6`). The C4 table row states the sign-off decision.
- [ ] `crates/validate/src/client.rs:358-361`: on PUT and DELETE, an error body shorter than its declared `Content-Length` is reported as a clean S3 error. That breaks the type's own promise at `crates/validate/src/error.rs:29` ("an error body shorter than its declared length" is `Unreadable`). Repro: a scripted `503` with `Transfer-Encoding: chunked` and `Content-Length: 500`, whose chunked body is a complete 56-byte `<Error><Code>SlowDown</Code><Message>m</Message></Error>`. DELETE gives `Service{503, Code("SlowDown"), message: Some("m")}`, and PUT gives the same. GET on the same bytes gives `Unreadable{… ContentLengthError { expected: 500, received: 56 }}`, because the SDK checks length only on GetObject. So one response gets two different classifications depending on the operation. This falls under the rubric's protocol-input class ("enforce declared `Content-Length`"). T4 already raised it; this confirms it with a reproduction. Fix: in `service_error`, compare the collected body length with any declared `Content-Length` (or refuse a response that carries both TE and CL) before trusting the SDK's reading. Add DELETE and PUT cases next to `an_error_body_cut_short_is_unreadable` (`crates/validate/tests/s3_client_roundtrip.rs:872`), which covers only the length-framed shape.
- [ ] `crates/validate/src/client.rs:298` / `:127`: error bodies are read into memory with no byte cap. The only bound is the operation deadline, which defaults to 300 s (`client.rs:79`). Repro: a scripted endpoint answers DELETE with a chunked `500` and then streams 1 MiB chunks, stopping at 768 MiB. With `operation_timeout = 20s` the call ends as `Timeout{Operation}`, and the process's peak memory (VmHWM) went from **19 MiB to 794 MiB**: the SDK held all 768 MiB before `service_error` ever ran. At the default deadline on a fast link that is many GiB. A broken or hostile endpoint can take the validator down with memory exhaustion, which is the failure the brief's "never holds … whole" reasoning exists to prevent. It also falls under the rubric's protocol-input class ("oversize input is … an error"). T4 flagged it; this measures it. Possible fix: an SDK interceptor (`modify_before_deserialization`) that wraps non-2xx response bodies in a size-limited body, so going over the cap surfaces as `Unreadable`. Add a regression that streams past the cap.
- [ ] T4's three blocking entries at `crates/validate/src/client.rs:496` claim that once hyper is stuck writing to a peer that stopped reading, the upload source and connection are held forever after the PUT returns. I could not reproduce this on the locked stack (hyper 1.10.1, aws-smithy-http-client 1.4.2), in three setups:
- [ ] base `deny.toml:77-86` and `deny-all-features.toml` (the RUSTSEC-2026-0253 waiver): the brief told Do to *rewrite* the waiver's rationale, keep its REMOVAL TRIGGER, and record the maintainer's acceptance of the exposure. The patch instead **deletes** the waiver from both files and pins `aws-sdk-s3 = "1.144.0"` (`Cargo.toml:94-98`), not the brief's `1.137.0`. This is correct on the facts. The **base** lockfile already resolves `aws-sdk-s3 1.148.0` → `lru 0.18.4`, and in the registry `aws-sdk-s3 1.142.0` depends on `lru ^0.16.3` while `1.144.0` depends on `^0.18.2`. So the waiver was already stale before this patch; its trigger had fired. `cargo deny check` is green (`gate-logs/C4-ci.log:3307-3311`). What follows from that: the RUSTSEC exposure the brief has sign-off §9 "confirm" no longer exists. The sentence mirrored onto #741 should say the advisory is not in the graph, not that an exposure was accepted. Deliverable (e) should be scored as superseded, not missing.
- [ ] `crates/validate/src/client.rs:349` (and the disclosure at `:33`): request ids are read with the SDK's `RequestId` reader, which prefers `x-amzn-requestid` when it is present. Repro: a 404 carrying both `x-amz-request-id: scripted-request-741` and `x-amzn-requestid: from-a-proxy` gives `request_id: Some("from-a-proxy")`, and the id the gateway stamped is lost. Criterion 2 names "the `x-amz-request-id` the gateway stamps", and
- [ ] `crates/validate/src/client.rs:496`: Upload cancellation only sets a flag and wakes the body poller. If an endpoint answers early and then stops reading while Hyper is blocked flushing a large upload, Hyper cannot reach the next body poll, so the connection task, socket and caller's source remain retained after `put_object` returns. The large-source regression uses `Then::Drain` (`crates/validate/tests/s3_client_roundtrip.rs:1169`), which removes this backpressure. Make cancellation release the source and terminate the connection independently of write readiness; test a large upload against a peer that keeps the socket open without reading.
- [ ] `crates/validate/src/client.rs:358`: Error-body validation runs after the SDK has collected the response without a byte limit. A faulty endpoint streaming a large non-success response can exhaust the validator's memory before the operation deadline expires; XML validation cannot prevent that allocation. Enforce a cumulative error-response byte budget before SDK aggregation, including chunked responses without a declared length, and add an oversized-response regression. The frozen T4 review corroborates this SDK behavior.
- [ ] `crates/validate/src/client.rs:361`: A well-formed error document is accepted without checking its collected length against the response's `Content-Length`. For PUT/DELETE, a response with chunked framing, a conflicting declared length and complete `<Error>` XML becomes `Service` despite the malformed framing; the SDK length enforcement relied on elsewhere covers GET only. Reject conflicting framing or enforce the declared length before trusting the error fields, and add PUT/DELETE regressions. The existing mismatch test exercises only successful GET bodies (`crates/validate/tests/s3_client_roundtrip.rs:920`); the frozen T4 review identifies the uncovered error-response cases.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_741/review-b
- [ ] The brief assumes a base that does not exist yet. It says the crate “DOES exist pre-patch” and must build on “#740's accepted result” (`brief.md:49-52`, `brief.md:67-69`), but the declared prerequisite is only `PLANNED` (`dependency-state.json:2-5`), and the resolved `origin/main` target has no `crates/validate` member at all (`Cargo.toml:9-32`). On this target, `cargo test -p wyrd-validate ...` cannot run. Revise the target/ordering to require a materialized #740 result rather than treating planned work as the pre-patch tree.
- [ ] The brief suppresses the tracker’s load-bearing dependency decision without tracker evidence. The only recorded maintainer comment says the shipped `aws-sdk-s3` move “needs the audit **before** the crate lands” and “Confirm before starting” (`notes.json:1`); the brief instead declares the exposure “SETTLED,” says its audit is merely a slice deliverable, and orders Do not to reopen it (`brief.md:10-14`, `brief.md:243-270`). Add the missing recorded acceptance or retain the human dependency/license decision as unresolved.
- [ ] Criterion 3 does not yet falsify the claimed *bidirectional client* memory invariant. Its permitted observables are the largest buffer requested from the PUT source or live generated PUT bytes (`brief.md:35-39`, `brief.md:221-226`); reading GET output incrementally can still pass if the client first collects the whole response and then yields small chunks. The target source guarantees only that the **gateway** streams (`crates/gateway-s3/src/lib.rs:12-17`), not that this new client does. Require a concrete aggregation-sensitive oracle for PUT and GET, and require the deliberate-buffering mutation to fail on each direction.
- [ ] The scope contains an optional drive-by change that breaks its own parallel-work claim. It says this slice touches `crates/validate/**`, root `Cargo.toml`, and `deny.toml`, disjoint from #738’s `crates/server/**` (`brief.md:70-76`), but later recommends deduplicating the server’s SDK dependency even though that is “not required by the criterion” (`brief.md:323-325`). The target’s pins are inline in `crates/server/Cargo.toml:126-130`, so deduplication necessarily adds the very `crates/server/**` edit the brief said was excluded. Remove that cleanup or declare the overlap and scope explicitly.
- [ ] size backstop — this slice is behaving oversized: patch is 101 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] `deny.toml` / `deny-all-features.toml` (the old RUSTSEC-2026-0253 entry, base `deny.toml:86`): the brief said to keep the waiver, rewrite its rationale, and record that the maintainer accepted an unsound `lru` in a shipped binary. The patch deletes the waiver instead. Checked: the deletion is correct. The base lockfile already resolves `aws-sdk-s3` 1.148.0 (`Cargo.lock:210-211`) and `lru` 0.18.4 (`Cargo.lock:2139-2140`). `aws-sdk-s3` 1.144.0+ requires `lru ^0.18.2`, while 1.142.0 still required `^0.16.3`. So the advisory matched nothing before this patch, and the waiver's own removal trigger had already fired. (I could not check 1.143.0, so the "first release" claim at root `Cargo.toml:94` is unverified. The 1.144.0 floor is safe either way.) What a human must decide: the brief's header asks sign-off §9 to confirm and post to #741 that the RUSTSEC-2026-0253 exposure was *accepted*. That sentence would now be false. No unsound `lru` ships. The tracker note should say the exposure is moot, and the departure from the brief should be recorded as deliberate.
- [ ] `crates/validate/src/client.rs:212-219`: **a complete chunked GET is refused** as `S3Error::Body` ("declared no Content-Length"). Reproduced: `200 OK, Transfer-Encoding: chunked`, body `3\r\nhel\r\n0\r\n\r\n` → `Err(Body{…})`. The refusal fixes the previous round's close-delimited torn-body finding, but it is wider than that finding needed. Chunked framing marks its own end, hyper already enforces the terminal chunk, and the rubric names "the chunked terminal CRLF" as acceptable framing. Real S3 and the Wyrd gateway always send `Content-Length`, so this only bites behind a proxy that re-frames responses. A human should decide whether the validator calls such a deployment "untrustworthy" or narrows the refusal to bodies framed only by connection close.
- [ ] `deny.toml:76` / `deny-all-features.toml:103` / root `Cargo.toml:94-98`: **the patch deletes the RUSTSEC-2026-0253 waiver instead of rewriting its rationale as the brief ordered** (`brief.md:356-374`: "replace" the rationale, record the 2026-08-17 acceptance, "leave the existing REMOVAL TRIGGER intact"). It also pins `aws-sdk-s3` at a 1.144.0 floor, not the 1.137.0 the brief named (`brief.md:352-354`). I checked the facts and the deviation looks correct. The base `Cargo.lock` already resolves only `lru 0.18.4` under `aws-sdk-s3 1.148.0`, so the waiver had already met its own removal trigger before this patch. In the local registry, `aws-sdk-s3` 1.142.0 depends on `lru 0.16.3` and 1.144.0 on `0.18.2`; 1.143.0 was not available locally to check. C4-ci shows `cargo deny` advisories ok on both configs. Still, the brief's premise ("aws-sdk-s3 pins `lru ^0.16.3`") was stale, so the sign-off item "confirm Do … replaced the waiver rationale" now has nothing to confirm. The maintainer's acceptance survives only as a Cargo.toml comment (`Cargo.toml:86`), and `deny-all-features.toml` sits outside the brief's stated file set. Sign-off should accept the deviation explicitly.

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
- Iteration delta (if iterating): Slice is oversized and not converging: 3 builds, blocking findings 13 -> 8 -> 9, patch 101 KB (over the 100 KB backstop). Every round fixes one set of hostile-response edge cases and the review finds the next. Core is sound: PUT/GET/DELETE round-trip, typed errors, and both-direction streaming oracles hold (both buffering mutations go red). Split in re-plan (pdca split 741): (1) client core + typed errors + streaming, as proven; (2) hostile-response hardening — byte cap on error bodies before SDK aggregation (768 MiB error body -> 794 MiB peak), declared Content-Length enforced on PUT/DELETE error responses, upload cancellation that releases the source/connection under write backpressure (disputed repro: confirm on a large-frame peer that stops reading), and the chunked-GET refusal / x-amz-request-id precedence calls. Brief correction: the RUSTSEC-2026-0253 waiver is moot. The lockfile already resolves aws-sdk-s3 1.148.0 -> lru 0.18.4, so the waiver deletion and the >=1.144.0 floor are correct. The tracker note must say the advisory is not in the graph, not that an exposure was accepted.
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
