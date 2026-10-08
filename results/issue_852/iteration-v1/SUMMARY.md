# Result — issue 852 / validate-s3-client-core

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: (the gap) On the base, `crates/validate` parses flags, resolves credentials,
  echoes them and exits (`crates/validate/src/lib.rs:5-8`). Its manifest says "Deliberately
  NO dependencies … the S3 client and its dependency audit arrive in their own slice (#741)".
- Success criterion: BINDING, shown by C4-verify in-process with no container and no
  network. The production client runs against a Wyrd S3 gateway served in-process on loopback
  from the crate's dev-dependencies:
  1. **Round trip.** PUT → GET → DELETE, the GET byte-identical. A GET after the DELETE
     reports the typed not-found error (404, `NoSuchKey`), checked field by field. An empty
     object also round-trips.
  2. **Typed error.** An error response surfaces as a value with the HTTP status, `<Code>`,
     `<Message>` and `x-amz-request-id`. The id must **equal** the header the relay saw the
     gateway send, not just have the right shape. No substring-matching of `Display`.
  3. **Streaming, both directions, bounded at every point.** Each direction has an oracle
     that holds for the WHOLE transfer, not at one moment, so a client that streams a prefix
     and then collects the rest fails it. The oracles sit on a loopback relay between the
     client and the gateway; it counts bytes per direction and paces the response. Do states
     the window `W` and the piece bound `K` below, with how each was derived (SDK re-chunking,
     hyper's buffers, loopback socket buffers). The payload must be at least `8 × W`; Do may
     shrink `W` by setting small socket buffers on the relay's own sockets, never by
     tuning the kernel.
     - **PUT, lag:** the generator is never materialised. At EVERY pull, bytes the source has
       produced minus bytes the relay has forwarded is at most `W`.
     - **PUT, retention:** every piece the source yields is a `Bytes::from_owner` value
       (`bytes 1.12.1` is on the lock; `from_owner` at `bytes-1.12.1/src/bytes.rs:254`) whose
       owner counts itself on drop. At every pull, at most `K` of the source's pieces are
       still alive. A client that keeps forwarded pieces fails this.
     - **GET, lag:** for the whole body, the relay writes at most `W` bytes past what the
       test has taken from the client's body stream. A client that waits for more than `W`
       before handing over a piece stalls, and the bounded wait turns that into a message,
       not a hang. Then every byte is compared.
     - **Reviewed, not tested:** a client that COPIES bytes it already forwarded or handed
       over into a buffer of its own and keeps the copy would pass the oracles above. In
       `build-notes.md`, Do states with `path:line` that neither body path has an
       accumulating buffer (no `collect`, `aggregate`, `into_bytes`, growing `Vec`/`BytesMut`),
       and Check's review checks it. A process-wide allocation counter cannot stand in: the
       in-process gateway allocates in the same process.
     - **Mutations.** Do runs five, records that each fires its assertion, reverts each, and
       reports them in `build-notes.md`: (i) buffer the whole PUT; (ii) forward the first
       MiBs of the PUT, then collect the rest; (iii) keep every forwarded PUT piece in a
       `Vec`; (iv) collect the whole GET; (v) hand over the first GET pieces, then collect
       the rest. Chunk counts or largest buffer seen are not acceptable for the GET leg:
       "collect, then re-chunk small" passes them.
  4. **Integrity and bounded waits.**
     - A PUT whose source ends short, runs long (including excess in a separate piece) or
       errors fails with the body error, and nothing is stored.
     - A GET whose connection the relay cuts mid-body fails with the body error, never as a
       shorter object.
     - Each deadline (connect, operation, body-idle) expires as a typed timeout naming its
       phase.
     - A request the SDK refuses to build (an empty key) is reported as exactly that.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
  - (a) A client built from `ResolvedConfig`, public in the **library** (`crates/validate/src/`)
    so the integration test can reach it: endpoint, region, static credentials, path-style,
    plain-HTTP connector, retries and stalled-stream protection **off**.
  - (b) The typed error in its **final shape**, with one variant per place a call can fail:
    an S3 error response; a response that cannot be read as S3; no response; a deadline,
    naming its phase; a request never built; an object body that failed or cannot be trusted.
    Its `<Code>` keeps "this code", "XML without a code" and "no body" apart. Children 2 and 3
    must be able to test against it without adding a variant.
  - (c) The request id read from the `x-amz-request-id` header itself. The SDK's generic
    accessor prefers `x-amzn-requestid` when both are present.
  - (d) Streaming PUT from a caller's source with a declared length, and a GET body read
    piece by piece.
  - (e) Connect, operation and body-idle deadlines, with defaults. Their tests need no
    invented bytes: a peer that accepts and never answers, a listener with a full accept
    queue, and the relay holding a real GET's tail.
  - (f) The dependency move (below).
  - (g) Update `docs/design/architecture/05-building-block-view.md:253` ("the S3 client
    arrives with #741").
  - (h) Test fixtures own every task they spawn and abort it on drop, unlike the detached
    spawn at `crates/server/tests/s3_http_wire.rs:88-90`.

  **/ out of scope:**
  - Every response a conforming S3 server does not send: empty-body errors, XML without a
    code, non-XML pages, broken or trailing-junk `<Error>` documents, unreadable 2xx, error
    bodies whose length disagrees with their framing, close-delimited or chunked GET framing,
    and the error-body byte budget (#853).
  - Every upload peer that acknowledges early or stops reading (#854).
  - Mark each site where those belong with an in-code `// deferred: #853` (responses) or
    `// deferred: #854` (uploads) marker, and answer a review finding on them with "Deferred —
    tracked in #853/#854". The rubric treats that as settled (`AGENTS.md`, "Deferrals are
    settled"). Implementing them here is how this slice grew to 103 KB.
  - Also out:
    - the 5 GiB property (#761);
    - TLS (`crates/gateway-s3/src/lib.rs:50-57`);
    - the capability matrix and `smoke` (#743), so the binary still echoes and exits;
    - any gateway change;
    - **any edit under `crates/server/`**.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 75 mutants tested in 3m: 11 missed, 20 caught, 44 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_852/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.43s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #852’s streaming S3 client for `wyrd-validate`: two implementation findings remain despite 11 passing integration tests and five independently detected streaming mutations.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The conforming-endpoint slice has measurable round-trip, error, streaming and deadline criteria, with malformed responses and early upload acknowledgements explicitly assigned to #853/#854 (`brief.md:12`, `brief.md:133`). |
| C2 Reproduction (red pre-fix) | N/A | This is a new API: reverting production while retaining the test reproduces missing symbols, not an executed behavioral failure (`reviewer-red.log:3`, `reviewer-red.log:229`); the verification exception is addressed under C4. |
| C3 Change | PASS | The patch stays within the client/dependency/documentation slice and preserves the agreed follow-up boundaries; no gateway or server source changes are present (`target/crates/validate/src/s3.rs:153`, `target/crates/validate/src/s3.rs:174`, `target/crates/validate/src/s3.rs:226`, `target/docs/design/architecture/05-building-block-view.md:253`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or decline the declared absence-based verification for a new API — 11 tests pass after restoration, but the pre-fix discriminator never executes and the remote-base coverage gate measures nothing (`reviewer-restored-green.log:18`, `gate-logs/C4-verify.log:243`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | The bounded-wait contract remains false for a continuously ready empty-piece source: one poll can prevent both the SDK deadline and cancellation from running; resolve R1 before claiming bounded execution (`target/crates/validate/src/s3/body.rs:99`, `reviewer-public-probe.log:5`). |
| T1 Structure | PASS | The normal dependency closure remains independent of Wyrd internals, while the real gateway is confined to test dependencies; the independently rerun blackbox guard passes (`target/crates/validate/Cargo.toml:24`, `target/crates/validate/Cargo.toml:42`, `reviewer-scanners.log:15`). |
| T2 Shape | PASS | The public error variants preserve the agreed failure distinctions, the implementation separates client/body/error responsibilities, and the existing crate root forbids unsafe code (`target/crates/validate/src/s3/error.rs:14`, `target/crates/validate/src/s3/error.rs:63`, `target/crates/validate/src/lib.rs:1`). |
| T3 Runtime | FAIL | A public PUT on a current-thread runtime outlives its 100 ms operation deadline and 500 ms outer timeout, requiring termination at 3 seconds; this is a reproduced runtime hang (`target/crates/validate/src/s3/body.rs:101`, `reviewer-public-probe.log:6`). |
| T4 Contribution | FAIL | The required multi-pass review has one distinct unresolved runtime finding, reported three times; R1 independently confirms it. The contribution-artifact subcheck is N/A until its mandatory publish rerun (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | The PUT oracle cannot establish the stated payload window because framing bytes receive payload credit; resolve R2 so a real lag above W fails the assertion (`target/crates/validate/tests/s3_client_roundtrip.rs:214`, `target/crates/validate/tests/s3_client_roundtrip.rs:572`, `reviewer-lag-probe.log:4`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this library-only, plain-HTTP client, exercised against redb/memory/local-FS, is fit for the next validator slices with #853/#854 pending — it does not yet validate a deployed system (`target/crates/validate/src/lib.rs:5`, `target/crates/validate/tests/s3_client_roundtrip.rs:279`, `brief.md:133`). |

All source citations above and below are grounded in the supplied `target/` (`$PDCA_TARGET`); evidence-log citations refer to this review sandbox. These are advisory judgments, not an acceptance decision.

**R1 — FAIL: empty upload pieces can hang the runtime and defeat deadlines.** At `target/crates/validate/src/s3/body.rs:99`, `poll_frame` repeatedly calls the caller's stream and immediately continues for an empty piece. An always-ready source can therefore keep this single poll running forever. The operation timeout configured at `target/crates/validate/src/s3.rs:96` cannot preempt it. This is a local source-consumption defect, not the early-answer/stopped-reading peer behavior deferred to #854.

The independent probe compiled the unchanged production body module: a nonempty piece returned immediately, while empty pieces required external termination (`reviewer-empty-probe.log:2`). A second probe used the public `S3Client`, the supplied real gateway fixture and a current-thread Tokio runtime. Neither the 100 ms operation deadline nor the 500 ms outer timeout returned before the process was terminated at 3 seconds (`reviewer-public-probe.log:1`). Bound the work performed in a poll and yield cooperatively with a wakeup; add a regression that checks deadline completion with continuously ready empty pieces. The three frozen batch-review reports describe this same defect and are counted once.

The public reproduction is runnable from this directory:

```sh
TMPDIR="$PWD/pdca-reviewer-852-temp" timeout 3s ./pdca-reviewer-852-probes/public_probe --exact reviewer_empty_piece_source_respects_timeout --nocapture
```

**R2 — FAIL: the PUT lag assertion understates outstanding object bytes.** `pump_up` counts every wire byte after the HTTP head, including aws-chunked framing (`target/crates/validate/tests/s3_client_roundtrip.rs:571`), but `PutProbe::observe` subtracts that number from generated object bytes (`target/crates/validate/tests/s3_client_roundtrip.rs:214`). If P object bytes and F framing bytes were forwarded, the measured lag is `produced - P - F`, not `produced - P`. The allowance therefore grows with transferred data, contrary to the fixed object-byte window required by `brief.md:29`. The test's comment acknowledges this relaxation at `target/crates/validate/tests/s3_client_roundtrip.rs:45`; the brief does not authorize it.

An independent arithmetic counterexample exercised the actual `PutProbe::observe`: after 16 MiB of payload and 23,040 framing bytes, actual outstanding payload of `W + 1 = 4,874,369` bytes was recorded as only `4,851,329`, so the existing `max_lag <= W` check accepts it (`reviewer-lag-probe.log:4`). This probe demonstrates the accounting gap, not a claim that the unmodified production client buffers that amount. Count forwarded object bytes, or conservatively exclude framing credit, and test the W boundary. The existing whole/prefix-buffering mutations remain useful but are too large to expose this undercount.

**The independent evidence supports the main client behavior, with the two boundedness limitations above.** The production change was stashed with the new integration test retained: Cargo exited 101 for absent client symbols and dependencies. After `git stash pop`, all 11 integration tests passed against the real SDK, TCP listener and Wyrd gateway (`reviewer-red.log:232`, `reviewer-restored-green.log:18`). No container, substitute CLI, external S3 service or network dependency was needed by that fixture. The supplied base is a single synthetic commit, so original integration-branch ancestry cannot be proved from its history; the prerequisite crate/guard are present and `git apply --reverse --check patch.diff` succeeds against the restored target (`reviewer-target-state.log:148`). No stale-target compile failure is being attributed to the patch.

The five required mutations were independently compiled in separate scratch copies, using the unchanged integration test and the already-built real dependencies. Each failed at its intended assertion, rather than failing to compile:

| Mutation | Observed failure | Evidence |
|----------|------------------|----------|
| Collect the whole PUT | 39,010,304-byte lag exceeds W = 4,874,368 | `reviewer-mutation-put_whole.log:10` |
| Forward a PUT prefix, then collect | 37,172,776-byte lag exceeds W | `reviewer-mutation-put_prefix.log:10` |
| Retain every forwarded PUT piece | 2,381 live owners exceeds K = 4 | `reviewer-mutation-put_retain.log:10` |
| Collect the whole GET | GET-lag timeout at offset 0 after relay credit is exhausted | `reviewer-mutation-get_whole.log:20` |
| Hand over a GET prefix, then collect | GET-lag timeout at offset 2,154,443 with exactly W bytes of outstanding credit | `reviewer-mutation-get_prefix.log:10` |

Review of both production body paths found no accumulating object buffer or retained copy: PUT moves each nonempty piece into its frame, and GET returns each received piece after counting its length (`target/crates/validate/src/s3/body.rs:103`, `target/crates/validate/src/s3/body.rs:216`). Deadline clocks consistently use Tokio; SigV4 wall-clock stamping is explicitly a separate lifecycle (`target/crates/validate/src/s3.rs:39`). No capability probe masking an eager initialization side effect was introduced. Malformed-response handling, unbounded SDK error-response collection and early upload success remain **Deferred — tracked in #853/#854**, as agreed; they are not re-raised here.

**The frozen gates have the following evidentiary limits.** Every named frozen log is present and was read; instance-scoped wrappers were not treated as missing target code.

- **C4-ci — PASS in the frozen run.** The log records successful checks, both advisory configurations, conformance and DST, ending in `all checks passed` (`gate-logs/C4-ci.log:3313`, `gate-logs/C4-ci.log:3914`). The independent rerun passed typos, docs lint/render, repository guards, fmt, workspace clippy/build/tests and dependency-use scanning, then stopped at the read-only advisory-cache lock (`reviewer-ci.log:23`, `reviewer-ci.log:579`, `reviewer-ci.log:3305`). The later conformance/DST steps therefore rely on the frozen run.
- **C4-verify — NEEDS-HUMAN.** Green is independently reproduced; the red leg never executes a test because the API is new (`gate-logs/C4-verify.log:243`). This is the C4 decision above.
- **C4-diff-cov — N/A, base-state caveat.** The gate cannot apply the stacked patch to `origin/main`; it produces no coverage measurement (`gate-logs/C4-diff-cov.log:10`). The current target compiles and matches the patch, so this is not a verification defect in the change.
- **C5-mutants — FAIL as recorded; survivors triaged.** The log shows 11 missed, 20 caught and 44 unviable mutants (`gate-logs/C5-mutants.log:13`). These are not 11 demonstrated production defects: response-error coverage belongs to #853; accessor/error-chain and body-hint survivors do not establish a scoped failure; the unconditional timeout classification survivor exposes missing connection-refusal coverage. The five required streaming mutations were separately rerun above.
- **T4-batch-review — FAIL, one distinct finding.** All three reports concern the same empty-piece loop; R1 independently reproduces it (`gate-logs/T4-batch-review.log:10`).
- **T4-contribution — N/A.** Contribution artifacts are deliberately absent during Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).
- **host-tikv — PASS in the frozen run.** Both real clippy commands complete successfully; this is compile evidence, not a TiKV service test (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`).

Independent formatting, documentation lint, blackbox dependency closure, diff whitespace and all-feature license/bans/source checks pass (`reviewer-scanners.log:1`). Both local advisory scans stop because Cargo's advisory database lock is outside the writable sandbox (`reviewer-scanners.log:19`, `reviewer-scanners.log:23`); the frozen logs actually show those scans succeeding (`gate-logs/C4-ci.log:3313`). This is a reviewer-host caveat, not a new dependency exposure or a patch failure. The dependency promotion/floor/waiver decisions are already accepted in the brief and are not reopened (`brief.md:187`, `target/Cargo.toml:74`).

**Prior art was checked by affected path.** GitHub's merged-main commit history was queried for every changed path, and all 19 closed/unmerged PRs were checked by their changed-file lists. Main has no `crates/validate` history; the shared paths show the existing SDK bump and dev-only waiver history. No closed/unmerged PR touches the validator or either deny configuration. The overlaps are unrelated dependency-update PRs; this patch changes no resolved dependency versions (`reviewer-prior-art-summary.log:1`). The known integration-only prerequisites explain the remote coverage mismatch. This investigation did not require another checkout or the withheld builder notes.

### Advisory — adversary

# Adversarial review — #852 validate-s3-client-core

Re-ran on a scratch copy of `$PDCA_TARGET` (patched tree): all 11 tests in
`crates/validate/tests/s3_client_roundtrip.rs` pass in about 5 s. I re-applied the brief's
five criterion-3 mutations myself (build-notes were withheld), plus three of my own. Overall
the streaming proof holds up. The real problems are one confirmed hang and three spots where
the tests would stay green while a public fact in the typed error went wrong.

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/body.rs:101` (`Some(Ok(piece)) if piece.is_empty() => continue`): this is the same bug T4-batch-review blocks on, and I measured what it does. Source used: `stream::repeat_with(|| Ok(Bytes::new()))`, declared length 10, a peer that reads and never answers, 1 s operation deadline. On a **multi-thread** runtime, `put_object` returns `Timeout { phase: Operation, limit: 1s }` after 1.001 s. But the hyper connection task keeps one worker spinning forever, and the test process never exits (killed by `timeout 60`, exit 124). That breaks the rubric's "spawned helper tasks are aborted on drop", because an abort cannot land on a task that never yields. On a **current-thread** runtime, the call hangs: neither the 1 s operation deadline nor an outer 5 s `tokio::time::timeout` fires (watchdog thread still waiting at 15 s). Passing empty frames through to the SDK does not fix this. `aws-runtime 1.10.0 src/content_encoding/body.rs:147-161` returns `Ready(Ok(true))` for an empty data frame, and the caller loop `continue`s, so the SDK spins the same way. The fix has to cap skipped empties per poll and then `cx.waker().wake_by_ref(); return Poll::Pending`. Add a current-thread test with exactly that source.
- NEEDS-HUMAN [impl] — `crates/validate/src/s3.rs:212` (`DispatchFailure(failure) if failure.is_timeout()`): C5's surviving mutant (guard replaced with `true`) shows no test ever produces `S3Error::NoResponse`. Under that mutant, a GET to a refused port would come back instantly as `Timeout { phase: Connect, limit: 5s }`, a deadline that never ran, and all 11 tests stay green. The production code is correct today: in `aws-smithy-http-client 1.4.2 src/client.rs:628-660` only smithy's `TimedOutError` (the connect timeout, since the read timeout is disabled) maps to a timeout, and a refused connection maps to `ConnectorError::io`. But scope (b) makes "no response" a final-shape variant that children #853/#854 build on, and invariant (b) says every reported fact is real. Add a test: bind a listener, read its port, drop it, GET, and assert `NoResponse`.
- NEEDS-HUMAN [impl] — `crates/validate/src/s3.rs:225` (`let request_id = request_id(raw);`): my mutation m6 swapped this for the SDK's generic `aws_sdk_s3::operation::RequestId::request_id(&err)`, which prefers `x-amzn-requestid`. **All 11 tests still pass.** Scope (c) exists only for the case where both headers are present, and nothing in the suite sends both. So `error_matches_wire` (`tests/s3_client_roundtrip.rs:769`) proves criterion 2's equality but not scope (c). The relay already splits and rewrites response heads in `pump_down`. Inject a decoy `x-amzn-requestid` there and assert the typed id still equals the captured `x-amz-request-id`. A proxy adding that header is not on the brief's #853 list of non-conforming responses.
- NEEDS-HUMAN [human] — `crates/validate/src/s3.rs:51-52` says "The body as a whole is bounded by its declared length times this." That is true, but it means nothing in practice. With the default 60 s body-idle deadline and a 5 GiB object, a peer that sends one byte every 59 s keeps a GET open for roughly 10^4 years, and no deadline fires. The operation deadline stops at the response head. This meets the rubric's per-await rule and the brief only asked for connect, operation and body-idle deadlines, so it is not a build defect. It is a scope call: should #743's scenario layer own a whole-transfer deadline, and should this comment say so instead of implying a bound?

## Notes (no action needed for this slice)

- `crates/validate/src/s3/body.rs:232` (`None if self.received == self.declared`): C5's surviving mutant (guard replaced with `true`) shows the patch's own short-body check is never exercised. `get_cut_mid_body_is_the_body_error_never_a_shorter_object` passes because hyper itself reports the early EOF under Content-Length framing (giving `Transport`). Without a Content-Length, the patch refuses up front (`LengthUndeclared`), so this guard can only fire on framing the brief puts in #853 (the deferral marker is at `s3.rs:404`). Settled by that deferral. Just know that "never a shorter object" is proven by hyper here, not by this code.
- PUT retention bound K is hit **exactly**: `max_live = 4 = K` on three clean runs. Lag peaked at 2.61–2.77 MiB against W = 4.87 MiB (about 45 % headroom). K comes from `aws-runtime 1.10.0`'s 64 KiB aws-chunked buffer. An SDK bump that changes chunking will turn `put_streams_within_the_window_at_every_pull` red with no client defect. Expect that on the next Dependabot bump.

## Attempted to refute; could not

- **The five mutations (criterion 3).** Re-applied each to `body.rs` and each goes red with its intended message. (i) Buffer whole PUT: `PUT lag … 39010304 … forwarded 0 … exceeds W = 4874368`. (ii) Forward 4 MiB then collect: lag 34941492 > W. (iii) Keep forwarded pieces: `2381 of the source's pieces were alive; K = 4`. (iv) Collect whole GET: `GET lag … handed over nothing for 30s at offset 0`. (v) Hand over 1 MiB then collect: stall at offset 1048576. The oracles run through the production `S3Client` → real SDK → real TCP → real `S3Gateway`. No parallel copy, no mock.
- **The review-only hole.** m7 (keep a private copy of every GET piece) passes, as the brief predicts. I read both paths: `s3.rs` and `s3/*.rs` contain no `collect`, `aggregate`, `into_bytes`, `Vec<`/`BytesMut` body buffer. The only growing value is the error-text `String` in `error_chain` (`body.rs:151-152`). The brief's "no copy kept" claim holds.
- **The dependency move.** The registry index shows `aws-sdk-s3` 1.143.0 still requires `lru ^0.16.3` and 1.144.0 requires `^0.18.2`, so the floor comment is right. `Cargo.lock` holds a single `lru 0.18.4`, so deleting the RUSTSEC-2026-0253 waiver is sound, and `C4-ci.log:3313-3319` shows `cargo deny` green on both configs. `cargo build -p wyrd-validate` alone (the shipped minimal features, with no feature unification from `wyrd-server`'s dev-dependencies) compiles. `cargo tree` gives 81 crates new to the shipped graph, 18 `aws-*`, including `rustls-native-certs`, `rustls-pki-types` and `openssl-probe`, all matching the root `Cargo.toml` comment. The blackbox guard is green (`C4-ci.log:27-28`).
- **Gate reds that are not evidence against the fix.** C4-verify `UNVERIFIABLE` is the outcome the brief predicts for net-new API: the reverted base cannot compile the test. C4-diff-cov "fail" comes from the patch not applying on `origin/main`, because Do builds on the integration branch. That is a stale-base artifact, not a coverage measurement. The other C5 survivors (`is_end_stream`, `size_hint`, `S3Client::deadlines`, `S3Error::source`, and the deleted `DispatchFailure(_)`/`ResponseError` arms, which fall through to the same result) are equivalent mutants or trivial accessors.

## Where the verdict leans on unearned claims

- The brief's "Built and exercised at Check: the whole client" overstates it. The `NoResponse` variant, the both-headers request-id rule (scope c) and the short-body guard are never exercised. The first two are cheap to add (see the [impl] bullets above).

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/body.rs:101`: An always-ready source yielding empty `Bytes` loops forever inside one `poll_frame` call. It never returns `Pending`, so cancellation cannot stop that task; on a single-worker runtime it also prevents the operation deadline from firing. Bound empty-piece processing per poll, wake the task and yield, and add a regression for an always-ready empty source. The frozen `T4-batch-review` log independently reports this defect.

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_roundtrip.rs:215`: The PUT lag oracle subtracts encoded wire bytes from produced payload bytes. `pump_up` counts aws-chunked framing as forwarded data (`crates/validate/tests/s3_client_roundtrip.rs:572`). With cumulative framing overhead H, the assertion permits actual payload lag up to W + H; H grows with the transfer, so excess buffering can pass the claimed fixed W bound. Count forwarded payload bytes separately from framing and use that count in the lag assertion.

No additional reuse, simplification or efficiency findings. Reviewed the target source and frozen gate evidence; no builds were rerun.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or decline the declared absence-based verification for a new API — 11 tests pass after restoration, but the pre-fix discriminator never executes and the remote-base coverage gate measures nothing (`reviewer-restored-green.log:18`, `gate-logs/C4-verify.log:243`, `gate-logs/C4-diff-cov.log:10`).
- [ ] C5 Causal adequacy — The bounded-wait contract remains false for a continuously ready empty-piece source: one poll can prevent both the SDK deadline and cancellation from running; resolve R1 before claiming bounded execution (`target/crates/validate/src/s3/body.rs:99`, `reviewer-public-probe.log:5`).
- [ ] T5 Judgment — The PUT oracle cannot establish the stated payload window because framing bytes receive payload credit; resolve R2 so a real lag above W fails the assertion (`target/crates/validate/tests/s3_client_roundtrip.rs:214`, `target/crates/validate/tests/s3_client_roundtrip.rs:572`, `reviewer-lag-probe.log:4`).
- [ ] Validation — fitness-to-purpose — Decide whether this library-only, plain-HTTP client, exercised against redb/memory/local-FS, is fit for the next validator slices with #853/#854 pending — it does not yet validate a deployed system (`target/crates/validate/src/lib.rs:5`, `target/crates/validate/tests/s3_client_roundtrip.rs:279`, `brief.md:133`).
- [ ] `crates/validate/src/s3/body.rs:101` (`Some(Ok(piece)) if piece.is_empty() => continue`): this is the same bug T4-batch-review blocks on, and I measured what it does. Source used: `stream::repeat_with(|| Ok(Bytes::new()))`, declared length 10, a peer that reads and never answers, 1 s operation deadline. On a **multi-thread** runtime, `put_object` returns `Timeout { phase: Operation, limit: 1s }` after 1.001 s. But the hyper connection task keeps one worker spinning forever, and the test process never exits (killed by `timeout 60`, exit 124). That breaks the rubric's "spawned helper tasks are aborted on drop", because an abort cannot land on a task that never yields. On a **current-thread** runtime, the call hangs: neither the 1 s operation deadline nor an outer 5 s `tokio::time::timeout` fires (watchdog thread still waiting at 15 s). Passing empty frames through to the SDK does not fix this. `aws-runtime 1.10.0 src/content_encoding/body.rs:147-161` returns `Ready(Ok(true))` for an empty data frame, and the caller loop `continue`s, so the SDK spins the same way. The fix has to cap skipped empties per poll and then `cx.waker().wake_by_ref(); return Poll::Pending`. Add a current-thread test with exactly that source.
- [ ] `crates/validate/src/s3.rs:212` (`DispatchFailure(failure) if failure.is_timeout()`): C5's surviving mutant (guard replaced with `true`) shows no test ever produces `S3Error::NoResponse`. Under that mutant, a GET to a refused port would come back instantly as `Timeout { phase: Connect, limit: 5s }`, a deadline that never ran, and all 11 tests stay green. The production code is correct today: in `aws-smithy-http-client 1.4.2 src/client.rs:628-660` only smithy's `TimedOutError` (the connect timeout, since the read timeout is disabled) maps to a timeout, and a refused connection maps to `ConnectorError::io`. But scope (b) makes "no response" a final-shape variant that children #853/#854 build on, and invariant (b) says every reported fact is real. Add a test: bind a listener, read its port, drop it, GET, and assert `NoResponse`.
- [ ] `crates/validate/src/s3.rs:225` (`let request_id = request_id(raw);`): my mutation m6 swapped this for the SDK's generic `aws_sdk_s3::operation::RequestId::request_id(&err)`, which prefers `x-amzn-requestid`. **All 11 tests still pass.** Scope (c) exists only for the case where both headers are present, and nothing in the suite sends both. So `error_matches_wire` (`tests/s3_client_roundtrip.rs:769`) proves criterion 2's equality but not scope (c). The relay already splits and rewrites response heads in `pump_down`. Inject a decoy `x-amzn-requestid` there and assert the typed id still equals the captured `x-amz-request-id`. A proxy adding that header is not on the brief's #853 list of non-conforming responses.
- [ ] `crates/validate/src/s3.rs:51-52` says "The body as a whole is bounded by its declared length times this." That is true, but it means nothing in practice. With the default 60 s body-idle deadline and a 5 GiB object, a peer that sends one byte every 59 s keeps a GET open for roughly 10^4 years, and no deadline fires. The operation deadline stops at the response head. This meets the rubric's per-await rule and the brief only asked for connect, operation and body-idle deadlines, so it is not a build defect. It is a scope call: should #743's scenario layer own a whole-transfer deadline, and should this comment say so instead of implying a bound?
- [ ] `crates/validate/src/s3/body.rs:101`: An always-ready source yielding empty `Bytes` loops forever inside one `poll_frame` call. It never returns `Pending`, so cancellation cannot stop that task; on a single-worker runtime it also prevents the operation deadline from firing. Bound empty-piece processing per poll, wake the task and yield, and add a regression for an always-ready empty source. The frozen `T4-batch-review` log independently reports this defect.
- [ ] `crates/validate/tests/s3_client_roundtrip.rs:215`: The PUT lag oracle subtracts encoded wire bytes from produced payload bytes. `pump_up` counts aws-chunked framing as forwarded data (`crates/validate/tests/s3_client_roundtrip.rs:572`). With cumulative framing overhead H, the assertion permits actual payload lag up to W + H; H grows with the transfer, so excess buffering can pass the claimed fixed W bound. Count forwarded payload bytes separately from framing and use that count in the lag assertion.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_852/review-b
- [ ] **The supplied base cannot support this slice.** `brief.md:9-11` says the base already has the validator skeleton; `brief.md:65-69` says this run builds on an integration branch containing #774/#775. The supplied target is instead `36f006d` (also local `main` and `origin/main`): `crates/validate` is absent, the workspace member list at target `Cargo.toml:9-32` excludes it, and target `xtask/src/repo_guard.rs:1-30` documents only the two earlier guards. Although `dependency-state.json:2-5` confirms #775 exists and is COMPLETE, its changes are not in this target. Consequently the promised verification command cannot exercise this package, and the brief's own stop condition (`brief.md:49-50`) applies. Revise the base/dependency instructions to identify and require a resolved commit containing both prerequisites; do not recreate them in this slice.
- [ ] **The streaming oracle proves early progress, not the promised memory invariant.** `brief.md:21-29` checks PUT forwarding only when the final source piece is requested and GET delivery only during one held tail. A client that forwards the required prefix, then collects the remainder passes these observations while allocating in proportion to object size. A client that retains copies while forwarding can even hold the whole object and pass. This contradicts the claimed evidence for `brief.md:51-56`, whose cited target contract explicitly requires memory independent of object size (`crates/core/src/write.rs:530-535`). The two whole-body collection mutations in `brief.md:30-33` do not test these counterexamples. Add an explicit client buffer bound with review/test evidence covering retention and the remainder, or narrow the invariant claimed as proven.
- [ ] **Load-bearing tracker decisions have no supplied evidence.** Neither `notes.json` nor `sources/` exists in this run. Thus the accepted split and three prior attempts (`brief.md:64-71`, `brief.md:145`), and especially the claimed dependency-promotion/floor/waiver approvals (`brief.md:150-152`), cannot be checked against the thread. This does not establish that approval was absent. Revise the brief to include attributable tracker excerpts or precise comment references supporting those decisions, so the revision/sign-off can verify them rather than treating the brief's own assertion as the record.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept or decline the declared absence-based verification for a new API — 11 tests pass after restoration, but the pre-fix discriminator never executes and the remote-base coverage gate measures nothing (`reviewer-restored-green.log:18`, `gate-logs/C4-verify.log:243`, `gate-logs/C4-diff-cov.log:10`).; C5 Causal adequacy — The bounded-wait contract remains false for a continuously ready empty-piece source: one poll can prevent both the SDK deadline and cancellation from running; resolve R1 before claiming bounded execution (`target/crates/validate/src/s3/body.rs:99`, `reviewer-public-probe.log:5`).; T5 Judgment — The PUT oracle cannot establish the stated payload window because framing bytes receive payload credit; resolve R2 so a real lag above W fails the assertion (`target/crates/validate/tests/s3_client_roundtrip.rs:214`, `target/crates/validate/tests/s3_client_roundtrip.rs:572`, `reviewer-lag-probe.log:4`).; `crates/validate/src/s3/body.rs:101` (`Some(Ok(piece)) if piece.is_empty() => continue`): this is the same bug T4-batch-review blocks on, and I measured what it does. Source used: `stream::repeat_with(|| Ok(Bytes::new()))`, declared length 10, a peer that reads and never answers, 1 s operation deadline. On a **multi-thread** runtime, `put_object` returns `Timeout { phase: Operation, limit: 1s }` after 1.001 s. But the hyper connection task keeps one worker spinning forever, and the test process never exits (killed by `timeout 60`, exit 124). That breaks the rubric's "spawned helper tasks are aborted on drop", because an abort cannot land on a task that never yields. On a **current-thread** runtime, the call hangs: neither the 1 s operation deadline nor an outer 5 s `tokio::time::timeout` fires (watchdog thread still waiting at 15 s). Passing empty frames through to the SDK does not fix this. `aws-runtime 1.10.0 src/content_encoding/body.rs:147-161` returns `Ready(Ok(true))` for an empty data frame, and the caller loop `continue`s, so the SDK spins the same way. The fix has to cap skipped empties per poll and then `cx.waker().wake_by_ref(); return Poll::Pending`. Add a current-thread test with exactly that source.; `crates/validate/src/s3.rs:212` (`DispatchFailure(failure) if failure.is_timeout()`): C5's surviving mutant (guard replaced with `true`) shows no test ever produces `S3Error::NoResponse`. Under that mutant, a GET to a refused port would come back instantly as `Timeout { phase: Connect, limit: 5s }`, a deadline that never ran, and all 11 tests stay green. The production code is correct today: in `aws-smithy-http-client 1.4.2 src/client.rs:628-660` only smithy's `TimedOutError` (the connect timeout, since the read timeout is disabled) maps to a timeout, and a refused connection maps to `ConnectorError::io`. But scope (b) makes "no response" a final-shape variant that children #853/#854 build on, and invariant (b) says every reported fact is real. Add a test: bind a listener, read its port, drop it, GET, and assert `NoResponse`.; `crates/validate/src/s3.rs:225` (`let request_id = request_id(raw);`): my mutation m6 swapped this for the SDK's generic `aws_sdk_s3::operation::RequestId::request_id(&err)`, which prefers `x-amzn-requestid`. **All 11 tests still pass.** Scope (c) exists only for the case where both headers are present, and nothing in the suite sends both. So `error_matches_wire` (`tests/s3_client_roundtrip.rs:769`) proves criterion 2's equality but not scope (c). The relay already splits and rewrites response heads in `pump_down`. Inject a decoy `x-amzn-requestid` there and assert the typed id still equals the captured `x-amz-request-id`. A proxy adding that header is not on the brief's #853 list of non-conforming responses.; `crates/validate/src/s3/body.rs:101`: An always-ready source yielding empty `Bytes` loops forever inside one `poll_frame` call. It never returns `Pending`, so cancellation cannot stop that task; on a single-worker runtime it also prevents the operation deadline from firing. Bound empty-piece processing per poll, wake the task and yield, and add a regression for an always-ready empty source. The frozen `T4-batch-review` log independently reports this defect.; `crates/validate/tests/s3_client_roundtrip.rs:215`: The PUT lag oracle subtracts encoded wire bytes from produced payload bytes. `pump_up` counts aws-chunked framing as forwarded data (`crates/validate/tests/s3_client_roundtrip.rs:572`). With cumulative framing overhead H, the assertion permits actual payload lag up to W + H; H grows with the transfer, so excess buffering can pass the claimed fixed W bound. Count forwarded payload bytes separately from framing and use that count in the lag assertion.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_852/review-b. 5 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
