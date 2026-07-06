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

# Check review — issue 364 / s3-http-wire-surface

**Task under review:** give the in-process gateway a real client-facing **S3-compatible HTTP endpoint** — bucket-scoped object **PUT / GET / DELETE** with **mandatory SigV4** auth over a loopback listener, mapping onto the existing `Gateway::put_object`/`get_object` client paths and adding a net-new `delete_object`, so the blueprint's day-one S3 round-trip can run over the wire (public-TLS/deployed stand-up deferred to #367).

_Grounding: patch verified applied at `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`; citations below resolve there. Cargo/`xtask ci` re-runs need an approval unavailable in this sandbox, so C4 grounds on the deterministic gate record in `check-gates.json` (a re-run artifact), not a fresh reviewer re-run._

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief is a well-anchored spec: binding = signed PUT→GET→DELETE round-trip byte-identical + unsigned/bad-sig refused (`brief.md:49-56`); scope, invariants, and pre-declared NEEDS-HUMANs are explicit. Decision it turns on: none — spec is unambiguous for the Check floor. |
| C2 Reproduction (red pre-fix) | PASS | Net-new coverage `crates/server/tests/s3_http_wire.rs` drives a real loopback listener; RED is structural (no `s3` module / no listener to dial before the patch). `check-gates.json` C4-verify records "red without the fix, green with it". Could not independently re-run (cargo needs approval). |
| C3 Change | PASS | Change maps each verb onto the existing seam (`put_object`/`get_object`, `crates/server/src/lib.rs:117,149`) and adds `delete_object` (`lib.rs:163`) over new `metadata::unlink` (`crates/core/src/metadata.rs:305`); wire layer stays generic over `Gateway<M,C,Co>` with concretes only at `cli::cmd_s3`. Reuses the client path rather than reimplementing it. |
| C4 Verification (red→green) | PASS | `check-gates.json`: C4-ci (gating) = pass (fmt/clippy/build/test/deny/conformance), C4-verify = pass red→green. Grounded on the gate artifact, not a reviewer re-run — cargo invocations were not approvable in this session; if the human wants an independent green, run `cargo test -p wyrd-server --test s3_http_wire` in `$PDCA_TARGET`. Not a blocking FAIL: gate is green and code grounds on target. |
| C5 Causal adequacy | PASS | Root cause is "no client-facing network endpoint" (`brief.md:38-43`); the fix adds the endpoint mapping onto the real client paths — it removes the cause, not a symptom. Symptom-guard smell-test does **not** fire: no capability probe (`hasattr`/try-import) or runtime guard around a present capability; `MAX_BODY_BYTES` is a resource limit, not a probe. Auth is fail-closed and verified before any gateway work (`s3/mod.rs:661-673`). |
| T1 Structure | PASS | New `crates/server/src/s3/{mod,sigv4,crypto}.rs` sits at the ADR-0010 wiring point; layer names no concrete backend (`s3/mod.rs:596-618`). Structurally coherent — but *which crate* it lands in is a pre-declared human call (see T5). |
| T2 Shape | PASS | Idiomatic axum router + fallback dispatcher; typed `AuthError`→S3 `<Error><Code>` mapping (`s3/sigv4.rs:808-833`), hand-rolled Clone to avoid a spurious `M/C/Co: Clone` bound (`s3/mod.rs:587-594`). No shape smells. |
| T3 Runtime | PASS | Test exercises a real loopback TCP listener end-to-end (`tests/s3_http_wire.rs:1294-1342`), signature produced by the production `sign()` and independently pinned to the AWS `get-vanilla` known answer (`s3/sigv4.rs:1136-1157`), so green is not self-referential. Gate CI (build/clippy/test) green. |
| T4 Contribution | PASS | Genuine load-bearing feature: the three tests assert byte-identical multi-chunk round-trip, unsigned→403-stores-nothing, and tampered-sig→403-stores-nothing (`tests/s3_http_wire.rs:1344-1395`) — flippable, not tautological. Advances the #367-gating goal. |
| T5 Judgment | NEEDS-HUMAN | Four security/scope judgment calls the human owns: (1) **hand-rolled SHA-256/HMAC** in the auth boundary (`s3/crypto.rs`) chosen to keep `cargo deny` green vs a RustCrypto dependency audit — vendored crypto on a security surface needs sign-off even though vectors are pinned; (2) **"Stream, don't buffer" invariant deviated** — the patch buffers the whole body up to 256 MiB (`s3/mod.rs:534,650`) because the `put_object` seam takes `&[u8]`, contra brief Scope/invariant (0015:789 OOM cliff); (3) **body is read before auth** (`to_bytes` at `s3/mod.rs:650` precedes `sigv4::verify` at :662), so an *unsigned* request can force up to 256 MiB allocation before rejection — a pre-auth memory-amplification gap the "fail-closed" invariant arguably should cover; (4) **TLS is modeled but unwired** (`TlsIdentity` carried, never bound; plaintext loopback at Check) — brief DEFERRED moves public-TLS to #367 and binding conditions omit TLS, but the "self-signed/test cert at Check" wording (`brief.md:62`) expected some cert. Also the pre-declared **crate-boundary** and **SigV4-scope/error-code-floor** calls (`brief.md:136-148`): builder landed inside `crates/server` and shipped a header-only SigV4 floor with no `X-Amz-Date` freshness/replay window. Prior-art on the affected paths (`s3/*`, `delete_object`, `unlink`) is net-new with no known conflicting closed work; confirm at sign-off. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must confirm the M4 floor is fit for purpose given the deviations above: is a **256 MiB-buffered, plaintext-loopback, header-only-SigV4, vendored-crypto** endpoint an acceptable at-Check deliverable, with true streaming + public-TLS + replay hardening deferred to #367 and a crypto/dependency audit? Binding Check criteria (signed round-trip byte-identical; unsigned/bad-sig refused) are met and gate-green; the decision is whether the deferred set is the right scope line, not whether the tests pass. |

### Advisory — adversary

# Adversarial review — issue #364 / s3-http-wire-surface

Skeptic's pass. I tried to refute the red→green proof, the fix, and the reviewer's
verdict. Findings below; `path:line` grounded on `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`). Advisory only — I gate nothing.

## Attacks that landed

- **NEEDS-HUMAN — the "Stream, don't buffer" invariant is inverted, not satisfied**
  (`crates/server/src/s3/mod.rs:51,167`). The brief lists streaming bodies **in Scope**
  ("**streaming** request/response bodies (no full-object buffering, per 0015:789)") and as
  a standing invariant ("Stream, don't buffer … no full-object buffering into the gateway
  heap"). The handler does the opposite: `to_bytes(body, MAX_BODY_BYTES)` materialises the
  **entire** request body into a `Bytes` before any gateway work, and GET buffers the whole
  object (`Gateway::get_object` returns `Vec<u8>`, `lib.rs:149`) into `Body::from(bytes)`.
  The 256 MiB cap does **not** address the cited OOM cliff — it is per-request, so a
  concrete failing case is *k* concurrent signed PUTs → up to *k*×256 MiB resident
  (e.g. 16 clients ≈ 4 GiB). The in-crate comment concedes the deferral, but the brief
  put streaming *inside* this issue's scope, not in the deferred set — a human must decide
  whether shipping a buffering-only floor satisfies the slice or is a scope miss.

- **NEEDS-HUMAN — the round-trip proves self-consistency, not S3 compatibility**
  (`crates/server/tests/s3_http_wire.rs:1239`; `crates/server/src/s3/sigv4.rs:163`). The
  binding success criterion is an "**S3-compatible** HTTP listener" that "an S3 client
  drives." The test signs with the gateway's **own** `sigv4::sign`, which shares
  `canonical_request` with `verify`, so any canonicalization bug is invisible to the
  round-trip — sign and verify would agree with each other while both diverging from AWS.
  The only *independent* anchor is `sigv4_get_vanilla_known_answer`, whose shape is `GET /`,
  **empty query**, two headers, empty body. That leaves the two canonicalization rules AWS
  actually requires — **sorted, percent-encoded canonical query string** and
  **URI-encoded** path — completely un-anchored: `canonical_request` interpolates the raw
  `query`/`uri` verbatim (`sigv4.rs:163`). Concrete failing case: a real `aws-sdk`/boto3
  request carrying query params (or unsorted/needing-encoding) computes its canonical query
  by sorting+encoding, the gateway computes it from the raw string → signatures diverge →
  the "S3-compatible" client is 403'd. No test exercises a non-empty query or a real SDK.
  A human should decide whether "S3-compatible" is met by self-consistent sign/verify at
  Check with real-SDK interop deferred to #367.

- **Concurrent DELETE is not idempotent, contradicting its own contract**
  (`crates/server/src/lib.rs:163-169`, `crates/core/src/metadata.rs:318-326`).
  `delete_object`'s doc-comment and the S3 handler (`mod.rs`, `Method::DELETE → 204`)
  promise idempotent success. But `unlink` CAS-requires the dirent unchanged
  (`metadata.rs:319`); two concurrent DELETEs of the **same existing key** both read the
  same dirent, the first commits (204), the second's `.require(dirent_key, dirent_bytes)`
  now fails → `CommitOutcome::Conflict` → `GatewayError::Conflict` → **HTTP 409
  OperationAborted**, not 204. A client that retries or races itself gets a spurious 409
  where real S3 returns 204. The single-threaded test never exercises this (only a
  same-key delete of a present-then-absent object, `s3_http_wire.rs:1323`).

- **UNSIGNED-PAYLOAD leaves the body outside the signature, and there is no TLS or
  freshness at M4** (`crates/server/src/s3/sigv4.rs:240`). With
  `x-amz-content-sha256: UNSIGNED-PAYLOAD` the canonical payload hash is the literal string,
  so the signature does **not** cover the body; `verify` also enforces no `x-amz-date`
  freshness/skew (conceded in the module doc). At M4 the listener is plaintext loopback
  (TLS deferred to #367), so a captured signed request is replayable indefinitely and, under
  UNSIGNED-PAYLOAD, its body is malleable. This is AWS-standard behaviour and pre-declared
  as deferred, but it is a real residual on the *M4 wire as shipped* worth naming at
  sign-off, since the "fail-closed auth" invariant is weaker than the prose implies once
  UNSIGNED-PAYLOAD is offered without a transport integrity layer.

- **The asserted RED is a compile-error red, not a behavioural one**
  (`check-gates.json` C4-verify "red without the fix"; `crates/server/tests/s3_http_wire.rs`
  new file importing `wyrd_server::s3`). Before the patch the `s3` module does not exist, so
  the test fails to **compile** — this proves the code is net-new, not that the assertions
  discriminate a correct implementation from a plausible-but-wrong one. C2 Reproduction is
  "none (no gate configured)." Acceptable for net-new coverage (the brief says so), but the
  reviewer's confidence in "red→green" should not be read as behavioural bisection.

## Attacks that did NOT land (attempted, could not refute)

- **Auth downgrade via header stripping** — I tried to bypass the signature by omitting
  `x-amz-content-sha256` or signing only `host`: `verify` forces `host` and `x-amz-date`
  into `SignedHeaders` (`sigv4.rs`, the downgrade guard), and the body hash is folded into
  the canonical request even in the `None` branch, so the body stays bound. Could not forge.
- **`unlink` CAS vs a concurrent overwrite** — an overwrite either rewrites the inode
  in place (caught by `.require(inode_key, …)`) or repoints the dirent (caught by
  `.require(dirent_key, …)`); both paths Conflict rather than dropping a live record. Sound.
- **Production-path exercise** — the round-trip genuinely goes through `axum::serve` →
  the real router → `handle` (`s3/mod.rs`), not a parallel harness. The path is real; my
  objection (above) is that the *signer* is in-tree, not that the *server* is bypassed.
- **SHA-256/HMAC correctness** — pinned to FIPS-180-4 / RFC 4231 / AWS get-vanilla vectors;
  I did not find a divergence.

### Advisory — codex

- NEEDS-HUMAN — `crates/server/src/s3/mod.rs:139` exposes the new gateway through `axum::serve` on a plain `TcpListener`; the `TlsIdentity` config is not wired into any HTTPS listener, and the test exercises raw HTTP. The brief defers public deployed TLS, but it also says Check uses a loopback self-signed/test cert, so sign-off should decide whether this satisfies the issue's TLS requirement.
- NEEDS-HUMAN — `crates/server/src/s3/mod.rs:167` materializes the whole request body with `to_bytes` before auth and before dispatch, then `crates/server/src/s3/mod.rs:203` passes that complete buffer into `put_object`. The 256 MiB cap avoids unbounded heap growth, but this is still a full-object buffering path rather than the brief's required streaming PUT surface.
- NEEDS-HUMAN — `crates/server/src/s3/sigv4.rs:163` and `crates/server/src/s3/sigv4.rs:260` feed the raw query string directly into the canonical request. AWS SigV4 canonicalization sorts and URI-encodes query parameters, so this can reject otherwise valid S3 clients once they send signed query parameters; the current tests self-sign with the same helper and do not cover that interoperability case.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec
- [ ] T5 Judgment — Four security/scope judgment calls the human owns: (1) **hand-rolled SHA-256/HMAC** in the auth boundary (`s3/crypto.rs`) chosen to keep `cargo deny` green vs a RustCrypto dependency audit — vendored crypto on a security surface needs sign-off even though vectors are pinned; (2) **"Stream, don't buffer" invariant deviated** — the patch buffers the whole body up to 256 MiB (`s3/mod.rs:534,650`) because the `put_object` seam takes `&[u8]`, contra brief Scope/invariant (0015:789 OOM cliff); (3) **body is read before auth** (`to_bytes` at `s3/mod.rs:650` precedes `sigv4::verify` at :662), so an *unsigned* request can force up to 256 MiB allocation before rejection — a pre-auth memory-amplification gap the "fail-closed" invariant arguably should cover; (4) **TLS is modeled but unwired** (`TlsIdentity` carried, never bound; plaintext loopback at Check) — brief DEFERRED moves public-TLS to #367 and binding conditions omit TLS, but the "self-signed/test cert at Check" wording (`brief.md:62`) expected some cert. Also the pre-declared **crate-boundary** and **SigV4-scope/error-code-floor** calls (`brief.md:136-148`): builder landed inside `crates/server` and shipped a header-only SigV4 floor with no `X-Amz-Date` freshness/replay window. Prior-art on the affected paths (`s3/*`, `delete_object`, `unlink`) is net-new with no known conflicting closed work; confirm at sign-off.
- [ ] Validation — fitness-to-purpose — Human must confirm the M4 floor is fit for purpose given the deviations above: is a **256 MiB-buffered, plaintext-loopback, header-only-SigV4, vendored-crypto** endpoint an acceptable at-Check deliverable, with true streaming + public-TLS + replay hardening deferred to #367 and a crypto/dependency audit? Binding Check criteria (signed round-trip byte-identical; unsigned/bad-sig refused) are met and gate-green; the decision is whether the deferred set is the right scope line, not whether the tests pass.
- [ ] `crates/server/src/s3/mod.rs:139` exposes the new gateway through `axum::serve` on a plain `TcpListener`; the `TlsIdentity` config is not wired into any HTTPS listener, and the test exercises raw HTTP. The brief defers public deployed TLS, but it also says Check uses a loopback self-signed/test cert, so sign-off should decide whether this satisfies the issue's TLS requirement.
- [ ] `crates/server/src/s3/mod.rs:167` materializes the whole request body with `to_bytes` before auth and before dispatch, then `crates/server/src/s3/mod.rs:203` passes that complete buffer into `put_object`. The 256 MiB cap avoids unbounded heap growth, but this is still a full-object buffering path rather than the brief's required streaming PUT surface.
- [ ] `crates/server/src/s3/sigv4.rs:163` and `crates/server/src/s3/sigv4.rs:260` feed the raw query string directly into the canonical request. AWS SigV4 canonicalization sorts and URI-encodes query parameters, so this can reject otherwise valid S3 clients once they send signed query parameters; the current tests self-sign with the same helper and do not cover that interoperability case.

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
- Iteration delta (if iterating): The binding Check floor passes (signed PUT->GET->DELETE byte-identical; unsigned/bad-sig -> 403; auth boundary survived forge attempts; crypto pinned to FIPS/RFC/AWS vectors) — keep all of that. But items 1-3 alone warrant iteration, and the rebuild should address all of the following: 1. "Stream, don't buffer" invariant is INVERTED and streaming was IN scope (not deferred). `to_bytes(body, MAX_BODY_BYTES)` (s3/mod.rs:167) materializes the whole request body (up to 256 MiB) before any gateway work, and GET buffers the whole object (`Gateway::get_object` -> Vec<u8>, lib.rs:149) into `Body::from`. The 256 MiB cap is per-request, so k concurrent PUTs -> k*256 MiB resident (the 0015:789 OOM cliff). Deliver true streaming PUT/GET (widen the `put_object`/`get_object` seam off `&[u8]`/`Vec<u8>` as needed) rather than a buffering-only floor. 2. Round-trip proves self-consistency, not S3 compatibility. The test signs with the gateway's own `sigv4::sign`, which shares `canonical_request` with `verify`, so canonicalization bugs are invisible. The sorted/percent-encoded canonical query string and URI-encoded path rules AWS requires are un-anchored — `canonical_request` interpolates raw query/uri (sigv4.rs:163,260). A real aws-sdk/boto3 request with query params diverges -> 403. Implement proper SigV4 canonicalization (sorted + URI-encoded query, URI-encoded path) and add an independent oracle: a known-answer vector with a non-empty/needs-encoding query, and ideally a real-SDK interop test. 3. Concurrent DELETE is not idempotent, contradicting its own contract. `unlink` CAS-requires the dirent unchanged (metadata.rs:319), so two concurrent DELETEs of the same key give first=204, second=409 OperationAborted where S3 returns 204. Make delete idempotent (treat the not-found/conflict-into-absent race as success) and add a concurrent/retry test. 4. Vendored hand-rolled SHA-256/HMAC on the auth boundary (s3/crypto.rs) was chosen to keep `cargo deny` green. Resolve the crypto-provenance decision: either run the RustCrypto dependency audit (ADR-0003 three-test + deny.toml allowlist) and use a vetted crate, or record explicit sign-off to keep the vendored implementation on a security surface. 5. TLS is modeled but unwired — `TlsIdentity` is carried but never bound; Check runs plaintext loopback, while the brief's "self-signed/test cert at Check" wording expected a cert. Wire the loopback TLS listener (public/deployed TLS remains deferred to #367). 6. Pre-auth memory amplification + replay residual: the body is read before `sigv4::verify` (s3/mod.rs:650 precedes :662), so an unsigned request can force up to 256 MiB allocation before rejection — verify auth before materializing the body (fits the streaming rework). Also add `x-amz-date` freshness/skew (replay window); note UNSIGNED-PAYLOAD leaves the body outside the signature on the plaintext wire. Net-new coverage red is a compile-error red (acceptable for a net-new module), but the assertions above must become behavioral (streaming, real-SDK canonicalization, concurrent DELETE). </content>
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
