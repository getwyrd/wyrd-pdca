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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 65 mutants tested in 3m: 1 missed, 20 caught, 44 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_852/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.20s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #852: the bounded, streaming `wyrd-validate` S3 client, typed errors, and SDK dependency promotion have no confirmed in-scope implementation defect; new-API verification and fitness still need human judgment.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The conforming-endpoint slice has falsifiable round-trip, wire-error, streaming, and timeout requirements, with non-conforming responses and upload peers explicitly assigned to #853/#854 (`brief.md:12`, `brief.md:133`). |
| C2 Reproduction (red pre-fix) | N/A | This is a new API, so there is no existing behavior to reproduce: stashing production changes while retaining the test yields missing-API/dependency compilation errors, not an executed behavioral red; disposition is recorded under C4 (`reviewer-red.log:2`, `brief.md:59`). |
| C3 Change | PASS | The patch respects the library-only scope and dependency boundary, updates the living architecture description, and changes neither the server nor gateway implementation (`target/crates/validate/Cargo.toml:21`, `target/docs/design/architecture/05-building-block-view.md:253`, `patch.diff:1`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or decline the declared absence-based verification for criteria 1, 2, and 4 — their pre-fix discriminator never executes, although all 14 restored tests pass and all five streaming mutations fail at their intended assertions; the remote-base coverage gate measured nothing (`gate-logs/C4-verify.log:316`, `reviewer-restored-green.log:21`, `reviewer-mutations.log:1`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | PASS | The streaming proof rejects whole-body buffering, prefix-then-buffering, and retained PUT pieces; source inspection finds no retained copy, and empty-source handling removes the non-yielding poll rather than probing an assumed capability (`reviewer-mutations.log:1`, `target/crates/validate/src/s3/body.rs:107`, `target/crates/validate/src/s3/body.rs:208`). |
| T1 Structure | PASS | The shipped client remains independent of Wyrd implementation crates, while dev-dependencies exercise the real gateway; the normal graph contains only `wyrd-validate` among Wyrd crates (`target/crates/validate/Cargo.toml:24`, `target/crates/validate/Cargo.toml:42`, `reviewer-integrity.log:4`). |
| T2 Shape | PASS | Callers can distinguish failure phases and inspect server facts as fields; the decoy-header test proves request-id precedence without parsing Display text, and formatting/lint checks passed (`target/crates/validate/src/s3/error.rs:14`, `target/crates/validate/src/s3.rs:262`, `target/crates/validate/tests/s3_client_roundtrip.rs:926`, `reviewer-ci.log:21`). |
| T3 Runtime | PASS | Real loopback execution covers integrity, three timeout phases, refused connections, and cancellation of an always-ready empty source on both runtime flavors; deadline ownership is consistently Tokio (`target/crates/validate/src/s3.rs:39`, `target/crates/validate/tests/s3_client_roundtrip.rs:1418`, `reviewer-restored-green.log:6`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; the substantive contribution audit must rerun at publish, so the deferred row requires no human clearance (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Path-based merged and closed-PR checks found no competing validator implementation; the remaining automatic mutant is in the explicitly deferred #853 framing class, and the recorded SDK promotion decision is not reopened (`reviewer-prior-art.log:1`, `reviewer-prior-art.log:29`, `gate-logs/C5-mutants.log:13`, `target/crates/validate/src/s3/body.rs:217`, `brief.md:187`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm this conforming-endpoint, plain-HTTP library slice is sufficient for the following validator slices — evidence exercises the real SDK and gateway with redb/memory/local-FS backends, while scenario execution, TLS, and production deployment composition are outside this slice (`target/crates/validate/tests/s3_client_roundtrip.rs:290`, `target/crates/validate/src/lib.rs:5`, `brief.md:143`, `brief.md:163`). |

No rebuild finding remains. The independent streaming counterexamples fail for the promised reasons, the previously reported implementation gaps have executable coverage, and the outstanding decisions concern the declared proof boundary and intended use.

The mutation rerun used the supplied disposable target and restored its original source after every case. Here `W = 4,874,368` bytes, the payload is `39,010,304` bytes, and `K = 4` source pieces. The relay credits decoded payload only (`target/crates/validate/tests/s3_client_roundtrip.rs:694`), and the consumer grants GET credit only after receiving and comparing each piece (`target/crates/validate/tests/s3_client_roundtrip.rs:1055`).

| Independent counterexample | Observed failure |
|---------------------------|------------------|
| Buffer the whole PUT | Lag 39,010,304 > W (`m1-put-whole.log:77`). |
| Stream a 2 MiB PUT prefix, then buffer | Lag 37,175,296 > W (`m2-put-prefix.log:10`). |
| Retain every forwarded PUT piece | 2,381 live pieces > K (`m3-put-retain.log:10`). |
| Buffer the whole GET | Consumer stalls at offset 0 with exactly W bytes forwarded (`m4-get-whole.log:10`). |
| Stream a GET prefix, then buffer | Consumer stalls at offset 2,101,499 with exactly W additional bytes forwarded (`m5-get-prefix.log:10`). |

The final unmutated run passes all 14 tests (`reviewer-restored-green.log:21`). Both body paths transfer pieces by value and keep scalar counters/error state rather than an accumulating body buffer (`target/crates/validate/src/s3/body.rs:84`, `target/crates/validate/src/s3/body.rs:166`). That last retained-copy property remains a code-review conclusion, as the brief requires; it is not claimed as a memory measurement. The patch reverses cleanly against the restored target (`reviewer-integrity.log:3`).

The gate evidence supports the following bounded conclusions:

- **C4-ci:** the frozen run completed all checks (`gate-logs/C4-ci.log:3923`). My rerun passed prose/docs, repository guards, formatting, clippy, workspace build/tests, and cargo-machete, then stopped at the read-only advisory-cache lock (`reviewer-ci.log:3308`). Copying the same advisory database into this review directory and changing only `advisories.db-path` let the real default and all-feature policy scans pass; no rule or waiver changed (`reviewer-deny-evidence.log:1`, `reviewer-deny-local.log:1`, `reviewer-deny-all.log:1`). Independent conformance, statics, and DST runs also passed (`reviewer-conformance.log:1`, `reviewer-statics.log:3`, `reviewer-dst.log:541`, `reviewer-dst.log:577`). The frozen deploy-guard result is clean (`gate-logs/C4-ci.log:3335`).
- **C4-verify:** my production stash/test/restore cycle reproduces the frozen compilation-only red and successful green. The compilation failure establishes API absence, not behavioral regression discrimination (`reviewer-red.log:2`, `gate-logs/C4-verify.log:316`).
- **C4-diff-cov:** the log contains no coverage measurement because it applies the patch to `origin/main`, which lacks the stacked prerequisites (`gate-logs/C4-diff-cov.log:10`, `brief.md:66`). The supplied target itself compiled and ran successfully. This is a base-state/coverage caveat, not a patch compilation or applicability defect.
- **C5-mutants:** the frozen automatic sweep reports 20 caught, 44 unviable, and one missed mutant (`gate-logs/C5-mutants.log:14`). The survivor makes the GET clean-EOF length guard unconditional. The conforming cut-body test reaches the transport-error arm; malformed-framing checks are explicitly deferred to #853 (`target/crates/validate/src/s3/body.rs:217`, `target/crates/validate/tests/s3_client_roundtrip.rs:1189`). That settled deferral is not re-raised. The five required buffering mutations were independently exercised above.
- **T4 batch review / host-tikv / contribution:** the frozen logs report zero blocking review findings, successful completion of both feature compilation commands, and publish-time deferral of contribution artifacts, respectively (`gate-logs/T4-batch-review.log:10`, `gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`, `gate-logs/T4-contribution.log:10`). I did not recreate the instance-scoped review wrapper or independently rerun the unrelated TiKV compilation.

The prior-art investigation queried all 356 closed/merged PRs and matched their changed files against every affected path, expanding the two truncated file lists. It also queried `main` history for `crates/validate`, which returned no commits, and inspected the open prerequisites #845/#849. Shared manifest history includes dependency bumps and the former waiver; it contains no closed or merged validator-client implementation (`reviewer-prior-art.log:1`, `reviewer-prior-art.log:5`, `reviewer-prior-art.log:27`). The normal dependency graph resolves `aws-sdk-s3 1.148.0` and `lru 0.18.4`, includes the documented certificate-discovery crates, and excludes `rustls` and other Wyrd crates (`reviewer-integrity.log:4`).

No external build tool or endpoint dependency remains undischarged for the declared slice: it was compiled and exercised against the real gateway, and the dependency scans used the real scanner and advisory database. SDK promotion, the version floor, and waiver deletion are already accepted in the brief; sign-off still owes the specified tracker record on #741 (`brief.md:187`, `brief.md:193`). No tracker message was sent. Build notes and unrelated checkouts were not consulted.

### Advisory — adversary

# Adversarial review — #852 (validate-s3-client-core), iteration 2

I re-ran the evidence myself at a scratch copy of `$PDCA_TARGET` (toolchain present, cargo 1.96, offline). The fix held against everything below except one classification gap.

## Refutation attempts

- **Red→green evidence: could not refute.** `cargo test -p wyrd-validate --test s3_client_roundtrip` passes 14/14 in about 5 s. C4-verify is `UNVERIFIABLE` as the brief predicted (the test calls new API). The red the brief asks for is the five mutations, so I wrote my own versions of them (switched by an env var, one build) instead of trusting build-notes. Each one fails a test with the brief's oracle:
  (i) buffer the whole PUT → `PUT lag: … produced 39010304 … forwarded 0 … exceeds W = 4874368`;
  (ii) forward 2 MiB then collect the rest → lag 37175296 > W;
  (iii) keep every forwarded piece → `PUT retention: 2381 … alive; K = 4`;
  (iv) collect the whole GET and (v) hand over 2 MiB, then collect → both stall at the paced relay and fail (`GET failed at offset 0` / `offset 2097152: the body-idle deadline of 10s expired`).
  These all run against the production path (`crates/validate/src/s3/body.rs:107-144`, `:208-248`), not a copy of it.
- **Round-1 regressions: could not refute.** Passing empty pieces through to the SDK (the old bug at `body.rs:114`) makes both runtime tests fail ("neither finished nor timed out within 30s"). Swapping `s3.rs` `request_id(raw)` for the SDK's generic accessor fails `error_response_fields_equal_what_the_gateway_sent` with `request_id: Some("decoy-x-amzn-requestid-852")`. Forcing the guard at `crates/validate/src/s3.rs:209` to `true` fails `a_refused_connection_is_no_response_not_a_timeout`. All three round-1 holes are now closed by tests that go red.
- **Oracle margins (measured over 3 runs).** PUT lag peaks at about 2.62–2.67 MiB against W = 4.65 MiB. Live source pieces hit exactly K = 4 every time, which follows from the aws-chunked buffer (`aws-runtime-1.10.0/src/content_encoding/body/http_body_1_x.rs`, the `WritingChunk` loop, 64 KiB `DEFAULT_CHUNK_SIZE_BYTE`). The relay's framing decoder credits exactly the payload (the test asserts this, and it holds). Neither oracle is a tautology.
- **Integrity edges the suite does not cover: could not refute.** I probed declared 0 with 1 extra byte, declared 0 with a source error, declared 0 with an empty piece then excess, declared 10 with a separate 1-byte excess, declared 10 then an error, and 10 bytes split around an empty piece. Every bad case returned the exact `BodyError` and the key stayed absent (404 `NoSuchKey`). The good case stored 10 bytes.
- **Diff coverage, which the gate did not measure (`gate-logs/C4-diff-cov.log`: patch does not apply on origin/main).** I ran `cargo llvm-cov -p wyrd-validate`. The only new production lines no test executes are: the `#853`-deferred branches (`s3.rs:173`, `:226-241`, `:250-254`, `body.rs:226-229`, `:243-246`); the `Display` impls in `error.rs`; `Deadlines::default` and `S3Client::new` (`s3.rs:57-63`, `:84-86`); the i64-overflow refusal (`s3.rs:128-132`); and three trivial re-entry paths (`body.rs:102`, `:153-156`, `:199`). Nothing outside a deferral marker is an untested decision. The surviving C5 mutant (`body.rs:239`) sits under the `// deferred: #853` marker at `body.rs:217`, so the rubric treats it as settled.
- **Dependency-move claims: could not refute.** In the registry index, `aws-sdk-s3` 1.143.0 still requires `lru ^0.16.3` and 1.144.0 requires `^0.18.2`, so the floor comment in the root `Cargo.toml` is exact. `cargo tree -p wyrd-validate -e normal`, diffed against every other workspace member's normal graph, gives exactly 81 new crates, matching the comment. It includes `rustls-native-certs`, `rustls-pki-types` and `openssl-probe`, and not `rustls`. `aws-sdk-s3` 1.148.0 does carry `#![forbid(unsafe_code)]`. `cargo deny` is green on both configs (`gate-logs/C4-ci.log:3322-3328`), and the lockfile gains only `wyrd-validate`'s edges.

## Findings

- NEEDS-HUMAN [human] — **A misconfigured endpoint is reported as `NoResponse`, the same variant as a refused connection.** `--endpoint` accepts any string (`crates/validate/src/args.rs:161`), and `crates/validate/src/s3.rs:249-255` sends every dispatch failure without a response to `S3Error::NoResponse`. I ran it: `127.0.0.1:PORT` (no scheme) and `ftp://127.0.0.1:1` both give `NoResponse { detail: "… ResolveEndpointError … was not a valid URI" }`, and `https://127.0.0.1:PORT` gives `NoResponse { … "invalid URL, scheme is not http" }`. In all three cases the relay saw 0 connections, so nothing reached the wire. Proposal 0017 §7 (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:489`) budgets transport failures as `availability`, so once #743 wires scenarios, a typo'd endpoint will read as a flaky deployment, not a config error. The brief puts "a request never built" in scope (b), and an endpoint the SDK refuses to resolve is that case. The `https` mapping is a documented, deliberate choice (`s3.rs:207`), which is why this needs a decision rather than a rebuild. One fix is to map `ResolveEndpointError` (and possibly a non-`http` scheme) to `RequestNotBuilt` in `classify`. The other is to refuse such endpoints at parse time in #774's surface. Not a non-conforming *response*, so not covered by the #853 deferral.
- Informational (no action needed for the brief) — the GET oracle reuses the PUT window (`crates/validate/tests/s3_client_roundtrip.rs:1034`, `Paced { window: w }`). Its terms (aws-chunked re-chunking, hyper's *write* buffer, the client *send* queue; `:147`) are PUT-path buffers. The test's own note at `:63-64` says any credit works for a streaming GET. Measured, the client was never more than 172–295 KB behind the relay, against W = 4.87 MB. So a client that holds up to about 4.5 MiB of a GET body still passes. The invariant (bounded regardless of object size) and mutations (iv)/(v) are still met, but a GET-specific credit around 1 MiB would make the oracle about 4× tighter. Also, in practice a collecting client fails through its own 10 s body-idle deadline, not the test's "GET lag" message.
- **Reviewed, not tested (brief criterion 3): confirmed.** Neither body path keeps a copy. `DeclaredLengthBody` (`body.rs:84-90`) holds only the source, counters and the failure slot, and hands each piece on by value (`:122`). `ObjectBody` holds only the stream, counters and a cloned error (`:166-173`), and returns each piece by value (`:232`). There is no `collect`, `aggregate`, `into_bytes` or growing `Vec`/`BytesMut` in either path.

Attempted to refute the empty-piece fix, the request-id fix, the connect/no-response split, the five streaming mutations, the integrity edges, the coverage gap and the dependency audit. I could not. The only open item is how endpoint misconfiguration is classified.

### Advisory — code-review

- No in-scope correctness bugs or actionable reuse, simplification, or efficiency findings. The upload and download paths pass pieces onward without accumulating copies (`crates/validate/src/s3/body.rs:118`, `crates/validate/src/s3/body.rs:223`); empty upload pieces yield cooperatively (`crates/validate/src/s3/body.rs:114`). The frozen CI evidence records all 14 integration tests passing.
- The sole surviving mutant changes the download end-length guard (`crates/validate/src/s3/body.rs:239`). Its non-conforming response coverage is explicitly deferred to #853 (`crates/validate/src/s3/body.rs:217`), so it is not re-raised as a finding.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept or decline the declared absence-based verification for criteria 1, 2, and 4 — their pre-fix discriminator never executes, although all 14 restored tests pass and all five streaming mutations fail at their intended assertions; the remote-base coverage gate measured nothing (`gate-logs/C4-verify.log:316`, `reviewer-restored-green.log:21`, `reviewer-mutations.log:1`, `gate-logs/C4-diff-cov.log:10`).
- [x] Validation — fitness-to-purpose — Confirm this conforming-endpoint, plain-HTTP library slice is sufficient for the following validator slices — evidence exercises the real SDK and gateway with redb/memory/local-FS backends, while scenario execution, TLS, and production deployment composition are outside this slice (`target/crates/validate/tests/s3_client_roundtrip.rs:290`, `target/crates/validate/src/lib.rs:5`, `brief.md:143`, `brief.md:163`).
- [x] **A misconfigured endpoint is reported as `NoResponse`, the same variant as a refused connection.** `--endpoint` accepts any string (`crates/validate/src/args.rs:161`), and `crates/validate/src/s3.rs:249-255` sends every dispatch failure without a response to `S3Error::NoResponse`. I ran it: `127.0.0.1:PORT` (no scheme) and `ftp://127.0.0.1:1` both give `NoResponse { detail: "… ResolveEndpointError … was not a valid URI" }`, and `https://127.0.0.1:PORT` gives `NoResponse { … "invalid URL, scheme is not http" }`. In all three cases the relay saw 0 connections, so nothing reached the wire. Proposal 0017 §7 (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:489`) budgets transport failures as `availability`, so once #743 wires scenarios, a typo'd endpoint will read as a flaky deployment, not a config error. The brief puts "a request never built" in scope (b), and an endpoint the SDK refuses to resolve is that case. The `https` mapping is a documented, deliberate choice (`s3.rs:207`), which is why this needs a decision rather than a rebuild. One fix is to map `ResolveEndpointError` (and possibly a non-`http` scheme) to `RequestNotBuilt` in `classify`. The other is to refuse such endpoints at parse time in #774's surface. Not a non-conforming *response*, so not covered by the #853 deferral.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] **The supplied base cannot support this slice.** `brief.md:9-11` says the base already has the validator skeleton; `brief.md:65-69` says this run builds on an integration branch containing #774/#775. The supplied target is instead `36f006d` (also local `main` and `origin/main`): `crates/validate` is absent, the workspace member list at target `Cargo.toml:9-32` excludes it, and target `xtask/src/repo_guard.rs:1-30` documents only the two earlier guards. Although `dependency-state.json:2-5` confirms #775 exists and is COMPLETE, its changes are not in this target. Consequently the promised verification command cannot exercise this package, and the brief's own stop condition (`brief.md:49-50`) applies. Revise the base/dependency instructions to identify and require a resolved commit containing both prerequisites; do not recreate them in this slice.
- [x] **The streaming oracle proves early progress, not the promised memory invariant.** `brief.md:21-29` checks PUT forwarding only when the final source piece is requested and GET delivery only during one held tail. A client that forwards the required prefix, then collects the remainder passes these observations while allocating in proportion to object size. A client that retains copies while forwarding can even hold the whole object and pass. This contradicts the claimed evidence for `brief.md:51-56`, whose cited target contract explicitly requires memory independent of object size (`crates/core/src/write.rs:530-535`). The two whole-body collection mutations in `brief.md:30-33` do not test these counterexamples. Add an explicit client buffer bound with review/test evidence covering retention and the remainder, or narrow the invariant claimed as proven.
- [x] **Load-bearing tracker decisions have no supplied evidence.** Neither `notes.json` nor `sources/` exists in this run. Thus the accepted split and three prior attempts (`brief.md:64-71`, `brief.md:145`), and especially the claimed dependency-promotion/floor/waiver approvals (`brief.md:150-152`), cannot be checked against the thread. This does not establish that approval was absent. Revise the brief to include attributable tracker excerpts or precise comment references supporting those decisions, so the revision/sign-off can verify them rather than treating the brief's own assertion as the record.
- [x] size backstop — this slice is behaving oversized: patch is 101 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [x] `crates/validate/src/s3.rs:51-52` says "The body as a whole is bounded by its declared length times this." That is true, but it means nothing in practice. With the default 60 s body-idle deadline and a 5 GiB object, a peer that sends one byte every 59 s keeps a GET open for roughly 10^4 years, and no deadline fires. The operation deadline stops at the response head. This meets the rubric's per-await rule and the brief only asked for connect, operation and body-idle deadlines, so it is not a build defect. It is a scope call: should #743's scenario layer own a whole-transfer deadline, and should this comment say so instead of implying a bound?

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (#852): a bad `--endpoint` (no scheme, `ftp://`, `https://`) surfaces as `S3Error::NoResponse` (`crates/validate/src/s3.rs:249-255`); map `ResolveEndpointError` / non-`http` scheme to `RequestNotBuilt` in `classify`, or reject at parse time (#774 surface), before #743 counts it as availability.
- Follow-up (#852): no whole-transfer deadline on GET (body-idle resets per byte); #743's scenario layer should own one, and the `crates/validate/src/s3.rs:51-52` comment should stop implying a bound.
