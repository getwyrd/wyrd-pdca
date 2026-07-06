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
