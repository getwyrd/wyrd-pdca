# Result — issue 364 / s3-http-wire-surface

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: 
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: an S3-compatible **HTTP listener** in the gateway role; **bucket-scoped**

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix — a net-new, spec-anchored feature landing behind the
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 364 / s3-http-wire-surface (iteration 2)

**Task under review:** the gateway has no client-facing network endpoint (it is an in-process
library + a `put`/`get` CLI client mode). Give it a real S3-compatible HTTP listener —
bucket-scoped object **PUT / GET / DELETE**, mandatory **SigV4** auth (fail-closed, no
anonymous access), **streaming** bodies, exercised over loopback at Check — so the blueprint's
day-one **byte-identical S3 round-trip** can run over the wire. This is UNSCHEDULED load-bearing
work gating the first-deployment gate (#367). Iteration 2 must additionally clear six
carry-forward defects (streaming not buffering; AWS-correct canonicalization anchored by an
external oracle; idempotent concurrent DELETE; crypto provenance; TLS; auth-before-body).

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Binding floor is well-formed and the change maps to it 1:1 — S3 listener + bucket-scoped PUT/GET/DELETE + mandatory SigV4 + byte-identical round-trip (`s3/mod.rs` handler; brief:48–56). Wire encoding is ILLUSTRATIVE per brief; nothing under-specified that blocks build. |
| C2 Reproduction (red pre-fix) | PASS | Net-new module → compile-error red is permitted by the brief (test file note, brief:108), but the added coverage is genuinely behavioral-falsifiable: `concurrent_delete_is_idempotent` reddens a 409-on-loser DELETE, the AWS-KAT `sigv4_*_known_answer` reddens a canonicalization divergence, `unsigned_put_is_refused_before_its_body_is_read` reddens auth-after-body. Not a bare compile red. |
| C3 Change | PASS | Verbs reuse the in-process client path (`put_object_streaming`/`get_object_streaming`/`delete_object`, `lib.rs:144,255,219` → `write::stream_write_data` `write.rs:406`, `read::read_chunk_verified` `read.rs:345`, `metadata::unlink` `metadata.rs:308`); wire layer stays generic over `Gateway<M,C,Co>` and names no concrete (ADR-0010 invariant held, `s3/mod.rs`). |
| C4 Verification (red→green) | PASS | Deterministic gates recorded green: `C4-ci` (xtask ci: fmt/clippy/build/test/deny/conformance) and `C4-verify` (red without fix, green with) in check-gates.json. Independent `cargo` re-run was blocked by this sandbox's approval wall; I instead grounded that the patch is applied in the target worktree and every cited seam resolves. Not a target-state caveat — the gate re-ran clean off the base. |
| C5 Causal adequacy | PASS | The three behavioral carry-forwards are root-cause fixes, not guards: buffering→true chunk-at-a-time streaming (seam widened off `&[u8]`/`Vec<u8>`), self-consistent sig→sorted/URI-encoded `canonical_query` pinned to AWS published vectors, non-idempotent DELETE→CAS-loser re-resolves to idempotent success. Symptom-guard smell-test: no capability probe / runtime guard around an optional capability — net-new feature, rule does not fire. |
| T1 Structure | PASS | New `crates/server/src/s3/{mod,sigv4,crypto}.rs` + `tests/s3_http_wire.rs`; landed **inside** `crates/server` (one of the two pre-declared crate-boundary options) — see T5, that choice is a human ratification, not a structural defect. |
| T2 Shape | PASS | Fail-closed auth boundary: `sigv4::verify` runs before body is touched (`s3/mod.rs` handler orders verify → `body.into_data_stream()`); constant-time signature compare; host+x-amz-date must be signed (downgrade-closed); 403 on any missing/malformed/skewed/mismatched input. Idiomatic axum fallback router. |
| T3 Runtime | PASS | Bounded memory by design: PUT re-chunks as bytes arrive, GET streams over a bounded mpsc(4) with backpressure, so peak resident is O(chunk_size) not O(object) — closes the 0015:789 OOM cliff. Auth precedes allocation, so an unsigned huge body is 403'd before materialisation. Could not execute the suite here (cargo blocked); design + recorded gate support it. |
| T4 Contribution | PASS | Delivers a genuine load-bearing capability (first client-facing network S3 endpoint) that gates #367 and is exercised at Check (signed round-trip green, unsigned/bad-sig 403). Not dead scaffolding. |
| T5 Judgment | NEEDS-HUMAN | Decision owed: ratify (a) keeping **hand-rolled SHA-256/HMAC on the auth boundary** vs adopting a vetted RustCrypto crate — impact: security-surface crypto provenance; the vendored code is pinned to FIPS/RFC-4231/AWS vectors (I confirmed the constants are the real published values) but "vetted-crate vs vendored on an auth boundary" is an ADR-0003 allowlist/deny.toml governance call; (b) the **crate-boundary** choice (landed in `crates/server` vs standing up `gateway-s3` §5:132); (c) the **SigV4-scope floor** (header-only, presigned deferred) and error-code subset. Each is a brief §Known-NEEDS-HUMAN. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: is the demonstrated floor fit for the first-deployment gate? Two points the human must weigh: (1) **TLS was re-deferred, not wired** — the prior sign-off (carry-forward item 5) asked for a loopback test cert; iteration 2 still runs **plaintext loopback**, justifying the re-deferral by the rustls crypto-provider license sitting outside deny.toml's allowlist (ISC not listed — I confirmed against `deny.toml`) = a human dependency decision. The brief's DEFERRED text said "at Check the listener is exercised over loopback with a self-signed/test cert," so plaintext-at-Check is a gap the human must accept or reject. (2) The live "gateway serving real S3 publicly over TLS" green is pre-declared off-Check to #367 (brief:57–62), not a defect here. SigV4 auth boundary — the load-bearing security floor — **is** exercised. |

## Notes for the human
- **No blocking defect found.** All five carry-forward *behavioral* items (streaming, AWS-correct
  canonicalization with an independent published-vector oracle, idempotent concurrent DELETE,
  auth-before-body, replay/skew window) are implemented and covered by falsifiable tests.
- **Sandbox limitation:** I could not execute `cargo test`/`xtask ci` in this review environment
  (command approvals were unavailable). C4 rests on the recorded deterministic gate results plus
  direct grounding of the applied patch in the target worktree. If a fresh independent gate re-run
  is cheap, re-run `crates/server` unit + `s3_http_wire` integration tests before sign-off.
- **Two carry-forward items were re-deferred rather than completed** (vendored crypto provenance,
  TLS wiring). Both are re-deferred *with* a coherent dependency-license rationale (ISC/OpenSSL-family
  licenses of ring/aws-lc-rs/rustls are outside the `deny.toml` allowlist), which is genuinely a
  human ADR-0003 decision — hence NEEDS-HUMAN above, not FAIL. The human should explicitly clear
  or reject these re-deferrals rather than let them ride a second time silently.

### Advisory — adversary

# Adversarial review — issue #364 / s3-http-wire-surface (iteration 2)

Skeptic's pass. I tried to refute the red→green evidence and the six carry-forward
"fixed" claims. The deterministic C4 gates (xtask ci, run-verify) pass and I do not
dispute them; my quarrel is with what the green *proves*. Findings below are grounded on
`$PDCA_TARGET = /home/eddie/wyrd/wyrd.pdca-wt-l1`.

## Refutations

- **NEEDS-HUMAN — The #1 prior-rejection ("stream, don't buffer") is implemented but never
  behaviorally demonstrated; the round-trip test cannot tell streaming from buffering.**
  The only wire round-trip (`crates/server/tests/s3_http_wire.rs:174`) PUTs a **55-byte**
  object with an **8-byte** chunk size. Every assertion in the suite (byte-identical
  round-trip, 200/204/404 codes) passes *identically* for a fully-buffering implementation
  — nothing bounds resident memory, and no large-object case exists. The structural
  streaming lives in `put_object_streaming`/`stream_write_data` and `get_object_streaming`
  (bounded channel, `crates/server/src/lib.rs`), but the *evidence* that iteration 1 was
  rejected for producing (a buffering floor) is not distinguished from the *evidence* that
  iteration 2 offers. A reviewer asserting "streaming delivered/demonstrated" is relying on
  code-reading, not the test. Concrete missing case: a PUT of an object ≫ `DEFAULT_CHUNK_SIZE`
  (1 MiB, `lib.rs:41`) with a peak-RSS or per-chunk-observation assertion — absent.

- **NEEDS-HUMAN — DELETE permanently leaks committed chunk fragments, and `unlink`'s own
  doc comment makes an unwarranted reclamation claim.** `crates/core/src/metadata.rs:305`
  states the orphaned fragments are "left as collectable garbage the pending-ledger sweep /
  GC reclaims." That is false for the delete path: the sweep (`sweep_expired_leases`,
  `crates/core/src/write.rs:461`) scans **only** `pending:` ledger keys, and a *committed*
  object's chunks were removed from the ledger at phase-4 release (`write.rs:275`). So a
  DELETE (net-new here) of a committed object orphans its fragments with **no** ledger entry
  and **no** mechanism that ever reclaims them — a monotonic on-disk leak per delete. The
  brief defers a chunk-store delete to a later milestone, but the *false* "GC reclaims" claim
  in the shipped comment is this diff's, and should be corrected or explicitly signed off.

- **NEEDS-HUMAN — Carry-forward item 2 ("real-SDK canonicalization") is proven only at the
  unit level; no request from a real SDK is ever exercised, and two concrete real-SDK inputs
  break.** The AWS query KAT (`sigv4.rs` `sigv4_aws_docs_example_sorts_query`) is a genuine
  independent oracle for `canonical_query` — credit where due — but the *wire* round-trip
  still signs with the gateway's own `sigv4::sign` and **never sends a query string**, so the
  end-to-end path is self-referential exactly as item 2 warned. Two concrete breaks a real
  client hits: (1) the canonical URI is the **raw** path (`crates/server/src/s3/mod.rs:165`
  → `canonical_request` `uri`, `sigv4.rs:254`) and `split_bucket_key` stores the key
  **still percent-encoded** (`mod.rs:192`), so a boto3 PUT to key `my file.txt` is stored/keyed
  as `my%20file.txt` — self-consistent round-trip, but a different object identity than S3;
  (2) aws-cli/boto3's default `x-amz-content-sha256: STREAMING-AWS4-HMAC-SHA256-PAYLOAD`
  (aws-chunked upload) is classified `PayloadHash::Signed("STREAMING-…")` (`sigv4.rs:356`),
  so the gateway would hash-mismatch (400) **and** store the chunk-framing bytes as object
  data. Item 2 asked "ideally a real-SDK interop test"; none was added.

- **NEEDS-HUMAN — Carry-forward item 5 (TLS) was not delivered: Check still runs plaintext
  loopback.** `TlsIdentity` is still an unbound seam — `S3Config::new` sets `tls: None`
  (`crates/server/src/s3/mod.rs:88`), `cmd_s3` never populates it, and `serve` binds a plain
  `tokio::net::TcpListener`. This is the same "modeled but unwired" state iteration 1 was
  dinged for. The rustls-provider-license NEEDS-HUMAN may be a legitimate blocker, but item 5
  asked to *wire the loopback TLS listener*, and it is not wired — the human should decide
  whether re-deferral is acceptable rather than treat item 5 as addressed.

- **NEEDS-HUMAN — Carry-forward item 4 (vendored crypto on the auth boundary) is re-deferred,
  not resolved.** The hand-rolled SHA-256/HMAC (`crates/server/src/s3/crypto.rs`) remains on
  the signature-verification path; the module doc re-declares it a pre-declared NEEDS-HUMAN.
  Item 4 asked to *either* run the ADR-0003 three-test RustCrypto audit *or* record explicit
  sign-off to keep the vendored code on a security surface. Neither the audit nor a recorded
  sign-off is in this diff — it is the same deferral with more prose. (The FIPS/RFC/AWS
  vectors are present and pass, so the implementation is plausibly correct; the open question
  is provenance/policy, which is exactly the human call.)

- **Replay within the 15-minute window is still open (residual, pre-declared).** The freshness
  bound (`MAX_CLOCK_SKEW = 15 min`, `sigv4.rs:32,338`) blocks stale signatures but there is no
  nonce/once-cache, so a captured signed PUT/DELETE is replayable for 15 minutes — and on the
  plaintext-loopback Check wire it is capturable. Brief flags this as a residual; noting it so
  the human weighs it, not as a surprise.

- **`UNSIGNED-PAYLOAD` leaves the body unauthenticated on the plaintext wire (residual,
  pre-declared).** For `PayloadHash::Unsigned` (`sigv4.rs:356`) `put_object_streaming` skips
  the body-hash check entirely, so on the plaintext Check wire an on-path attacker can
  substitute the body of an UNSIGNED-PAYLOAD PUT. Pre-declared in the brief; real until TLS
  is wired (see the TLS finding above — the two compound).

## Attempted but could not refute

- **Auth-before-body (item 6).** I tried to show the body is materialized before auth: it is
  not. `handle` runs `sigv4::verify` on request parts before touching `body`
  (`mod.rs` handler), and `unsigned_put_is_refused_before_its_body_is_read` genuinely proves
  it — a 1 GiB declared / 0-sent unsigned body gets a prompt 403 under a 10 s timeout. This
  assertion is behavioral and sound.
- **Concurrent DELETE idempotency (item 3).** I tried to find an interleaving that yields 409:
  `delete_object`'s Conflict→re-resolve→`Ok(false)` branch plus the dirent+inode double-CAS in
  `unlink` closes the delete-vs-delete race, and the overwrite race is retried (bounded 8x).
  The `concurrent_delete_is_idempotent` test races two spawned tasks on a multi-thread runtime
  over 64 rounds and asserts "exactly one removes, object ends gone" — correct for every
  interleaving I could construct. Sound.
- **SigV4 correctness of the crypto chain.** The `get-vanilla` and IAM worked-example KATs pin
  the full canonical-request→string-to-sign→signing-key→HMAC chain to AWS's published
  signatures; I could not construct a divergence for the header-signed, no-query floor.

### Advisory — codex

- NEEDS-HUMAN — crates/server/src/s3/mod.rs:141: `S3Gateway::serve` always hands a plain `TcpListener` to `axum::serve`, and the CLI path similarly binds plaintext at crates/server/src/cli.rs:756. The brief’s carry-forward item expected a loopback TLS listener with a self-signed/test cert at Check, while the patch explicitly defers TLS binding behind a dependency/license decision. Human sign-off should decide whether plaintext loopback is acceptable for this slice or whether the check floor still requires wired TLS.
- crates/server/src/s3/sigv4.rs:371: SigV4 verification feeds the request path into the canonical request verbatim, while only the query string is normalized/encoded. The tests cover simple paths, but real S3 clients canonicalize object keys with reserved characters using URI encoding rules; a request for a key such as `space key`/`a+b`/non-ASCII can diverge from this verifier and fail with 403 even though the same SDK signs it correctly.
- crates/server/src/s3/mod.rs:245: XML error bodies interpolate `message` without escaping. Auth errors can include attacker-controlled `SignedHeaders` text via crates/server/src/s3/sigv4.rs:366, so a malformed Authorization header can produce invalid XML or injected error markup instead of a well-formed S3 error response.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Decision owed: ratify (a) keeping **hand-rolled SHA-256/HMAC on the auth boundary** vs adopting a vetted RustCrypto crate — impact: security-surface crypto provenance; the vendored code is pinned to FIPS/RFC-4231/AWS vectors (I confirmed the constants are the real published values) but "vetted-crate vs vendored on an auth boundary" is an ADR-0003 allowlist/deny.toml governance call; (b) the **crate-boundary** choice (landed in `crates/server` vs standing up `gateway-s3` §5:132); (c) the **SigV4-scope floor** (header-only, presigned deferred) and error-code subset. Each is a brief §Known-NEEDS-HUMAN.
- [ ] Validation — fitness-to-purpose — Decision owed: is the demonstrated floor fit for the first-deployment gate? Two points the human must weigh: (1) **TLS was re-deferred, not wired** — the prior sign-off (carry-forward item 5) asked for a loopback test cert; iteration 2 still runs **plaintext loopback**, justifying the re-deferral by the rustls crypto-provider license sitting outside deny.toml's allowlist (ISC not listed — I confirmed against `deny.toml`) = a human dependency decision. The brief's DEFERRED text said "at Check the listener is exercised over loopback with a self-signed/test cert," so plaintext-at-Check is a gap the human must accept or reject. (2) The live "gateway serving real S3 publicly over TLS" green is pre-declared off-Check to #367 (brief:57–62), not a defect here. SigV4 auth boundary — the load-bearing security floor — **is** exercised.
- [ ] crates/server/src/s3/mod.rs:141: `S3Gateway::serve` always hands a plain `TcpListener` to `axum::serve`, and the CLI path similarly binds plaintext at crates/server/src/cli.rs:756. The brief’s carry-forward item expected a loopback TLS listener with a self-signed/test cert at Check, while the patch explicitly defers TLS binding behind a dependency/license decision. Human sign-off should decide whether plaintext loopback is acceptable for this slice or whether the check floor still requires wired TLS.

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
- Iteration delta (if iterating): Rejected: the demonstrated floor is not yet fit for the first-deployment gate. The rebuild must CORRECT the following, not re-defer them a second time. T5 governance (all three must be corrected): - (a) crypto provenance: replace the hand-rolled SHA-256/HMAC on the auth boundary with a vetted RustCrypto crate — run the ADR-0003 three-test audit / update deny.toml. Silent re-deferral is not acceptable again. - (b) crate-boundary: pick and commit the S3 wire layer's home (gateway-s3 vs crates/server) rather than leaving it an open ratification. - (c) SigV4 scope: correct to the required floor (header-only / error-code subset), addressing the real-SDK gaps named below. Adversary findings to fix in the rebuild: - Streaming is implemented but never behaviorally demonstrated: the only wire round-trip is 55 bytes / 8-byte chunk and passes identically for a buffering impl. Add a PUT of an object > DEFAULT_CHUNK_SIZE (lib.rs:41) with a bounded-memory / per-chunk-observation assertion. - DELETE leaks committed chunk fragments AND unlink's doc comment (crates/core/src/metadata.rs:305) falsely claims "GC reclaims" — the sweep scans only pending: keys. Correct the false comment. - Canonicalization is proven only at unit level; no real-SDK request is exercised. Fix the two named real-client breaks: percent-encoded key identity (boto3 key "my file.txt" stored/keyed as "my%20file.txt") and STREAMING-AWS4-HMAC-SHA256- PAYLOAD misclassification (400 + chunk-framing stored as object data). Add a real-SDK interop path. - XML error bodies (codex) interpolate `message` without escaping; attacker- controlled SignedHeaders can inject markup. Escape the error message. Accepted — do NOT spend iteration budget here: - TLS: plaintext-loopback-at-Check is accepted; TLS wiring comes later (human decision). Do not re-wire TLS this iteration. - Replay-within-15-min and UNSIGNED-PAYLOAD-on-plaintext are pre-declared residuals tied to the accepted TLS deferral.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
