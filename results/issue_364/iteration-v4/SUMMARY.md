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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
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

**Task under review:** give the gateway its first client-facing network endpoint — an
S3-compatible HTTP listener serving bucket-scoped object **PUT / GET / DELETE** with
mandatory SigV4 auth and **streaming** bodies, mapping onto the existing in-process
client paths, so the blueprint's day-one signed round-trip can run over the wire
(brief.md:44-56). This is **iteration 4**; iterations 1-3 were rejected for a
buffering-only floor, self-referential SigV4, non-idempotent/leaking DELETE, hand-rolled
crypto, and a red workspace gate (brief.md:162-176).

**Grounding note (target-state caveat, not a patch defect):** `$PDCA_TARGET` is
unreadable in this sandbox, so per the reviewer discipline every citation below is
grounded on `patch.diff` alone; I did not use any other checkout on the machine. I could
not myself re-run the workspace gate against an authorized clean target.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion is concrete and demonstrable-at-Check: signed PUT→GET→DELETE byte-identical, unsigned/bad-sig refused, streaming bodies (brief.md:48-56); live public-TLS is explicitly DEFERRED to #367 (brief.md:57-62), so the Check floor is well-bounded. Nothing to decide here. |
| C2 Reproduction (red pre-fix) | PASS | C4-verify gate recorded RED-without-fix / GREEN-with-fix (check-gates.json:42-48, run-verify.sh); net-new module ⇒ compile-error red is acceptable per brief.md:108. The behavioral asserts (streaming, concurrent-delete, placement, percent-key) are genuine regressions, not self-referential (patch.diff:2728-3240). |
| C3 Change | PASS | Wire surface reuses the in-process seam rather than reimplementing it: PUT→`put_object_streaming` (patch.diff:957), GET→`get_object_streaming` (patch.diff:1123), DELETE→`delete_object` (patch.diff:1043); concretes wired only at `cli::cmd_s3` (patch.diff:836), honoring ADR-0010. |
| C4 Verification (red→green) | FAIL | **Gating** C4-ci is RED: `cargo test --workspace --exclude wyrd-dst` exit 101, overall=fail (check-gates.json:32-40). This is the sole hard blocker and mirrors iteration-3's red, which the prior adversary re-ran clean and attributed to a stale/flaky timing test (gateway_lease_expiry.rs / gateway_cluster.rs), NOT the diff (brief.md:173). **Decision owed:** re-run the workspace gate and *localize* the failing binary — accept only if it is the known pre-existing flake and re-runs green; do NOT lean on the green per-fix verify (check-gates.json:42-48), which exercises only this patch's new test. I could not re-run it here (no readable/authorized target). |
| C5 Causal adequacy | PASS | Fixes remove/transform causes, they do not guard symptoms — symptom-guard smell-test does NOT fire: no capability probe or runtime guard papering a load-time side effect (the `Streaming`→501 at patch.diff:1590 is a scope decision; `delete_fragment_at` at patch.diff:3258 is a trait default). Streaming replaces buffering (patch.diff:478,957,1123); crash-leak closed by an orphan grace record written in unlink's atomic commit (patch.diff:292-338); DELETE reclaim is placement-aware (patch.diff:1087-1105). **Watch item for the human:** the streaming PUT path uses *identity* placement, not the failure-domain selector, a disclosed "later refinement" (patch.diff:431-432) — internally consistent (delete/read read the stored placement) but a behavioral divergence from the buffered path. |
| T1 Structure | PASS | Wire layer lands as `crates/server/src/s3` submodule (crypto/sigv4/mod), generic over `Gateway<M,C,Co>` naming no concrete (patch.diff:1497-1543); orphan-key protocol centralized in `core::metadata` so delete-writer and GC-reader cannot key-drift (patch.diff:214-243, 545). Coherent placement. |
| T2 Shape | PASS | New surfaces are backward-compatible: `PlacementChunkStore::delete_fragment_at` has a default delegating to `delete_fragment` (patch.diff:3258-3260) so existing stores are unaffected; `Unlinked`/`PayloadHash`/`AuthError` are well-typed; RustCrypto promoted as a direct dep already in-graph via tonic (patch.diff:160-191). |
| T3 Runtime | PASS | Behavior is exercised at runtime, not just asserted structurally: real loopback listener round-trip (patch.diff:2728), auth-before-body (patch.diff:2808), wrong-sig stores nothing (patch.diff:2837), concurrent-delete idempotency over 64 rounds on a multi-thread runtime (patch.diff:2877), streaming first-write-before-drain (patch.diff:3053), GC crash-leak backstop (patch.diff:695). Caveat: gated by the red C4-ci above. |
| T4 Contribution | PASS | Net-new, load-bearing wire surface that gates the first-deployment gate #367 (brief.md:71-72); addresses every enumerated iteration-1/2/3 carry-forward (streaming, real canonicalization + AWS known-answer oracle patch.diff:2331/2361, idempotent+placement-aware DELETE, orphan ledger, RustCrypto, XML escaping, percent-key, pre-auth body). |
| T5 Judgment | NEEDS-HUMAN | Four pre-declared human-only calls remain owed and are the crux of sign-off, not restated code: **(a) crypto provenance** — the hand-rolled MAC is replaced with RustCrypto sha2/hmac claiming an ADR-0003 three-test audit + deny.toml allowlist (patch.diff:174-191, 1221-1235); the human must confirm the audit was actually run and `cargo deny` is green (part of the red C4-ci). **(b) crate boundary** — builder *committed* to `crates/server` over the named `gateway-s3` crate (patch.diff:1391-1399); ratify or reverse. **(c) SigV4 scope / error-code floor** — header-only, presigned + aws-chunked out (501), a minimal error-code set (brief.md:136-148); confirm the floor. **(d) real-SDK interop still unproven** — the round-trip signs with the gateway's own `sign()` (patch.diff:2640); AWS published known-answer vectors de-risk canonicalization but no actual boto3/aws-sdk client drives the listener (iteration-3 open item, brief.md:173). Also: sequencing (M4 branch vs own sequence, brief.md:143-145) and prior-art-by-affected-path beyond the recorded v1-v3 iterations cannot be settled mechanically here. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human owns fitness of the Check floor to the first-deployment gate. The BINDING conditions are met in code and tested over loopback, but the LIVE "gateway serving real S3 to a public client over TLS" is pre-declared DEFERRED to #367 (brief.md:57-62,157-160); TLS is modeled (`TlsIdentity`) but unwired, an accepted deferral per iteration-2 carry-forward (brief.md:168). The human must judge whether the plaintext-loopback demonstrated floor is fit to gate #367 — and this cannot be accepted while the gating C4-ci gate is red. |

### Advisory — adversary

# Adversarial review — issue #364 / s3-http-wire-surface (iteration 4)

Advisory only; never gates. Grounded on the target at
`/home/eddie/wyrd/wyrd.pdca-wt-l1` (patch applied to working tree). I re-ran the
evidence and tried to break the fix; below is what survived and what didn't.

## Findings

- **NEEDS-HUMAN — Concurrent GET-during-DELETE bypasses the reader-safe grace window the
  code itself cites.** `Gateway::delete_object` reclaims fragments *eagerly and immediately*
  (`crates/server/src/lib.rs:276`–`285`, `delete_fragment_at` on the winning commit), while
  `get_object_streaming` returns a `ReceiverStream` fed by a spawned reader over a
  bounded(4) channel (`crates/server/src/lib.rs:321`,`324`,`326`). Concrete failing case: a
  client GETs a multi-chunk object (>4 chunks) over a slow/backpressured socket, so the
  reader task blocks on `tx.send` with several chunks still unread; a second client DELETEs
  the same key; the eager reclaim removes *all* fragments of the not-yet-read chunks; the
  reader's next `read::read_chunk_verified` (`crates/core/src/read.rs:345`) errors
  (RS(6,3): all fragments gone ≫ m=3), so the already-`200`-status chunked GET terminates
  mid-object with an error / a body missing its terminating 0-chunk (silent truncation for
  a client that doesn't validate framing). The orphan **grace window** (`0005:291-294`) that
  exists precisely to keep in-flight reads safe *is* written to the ledger by `unlink` but
  the happy-path eager reclaim ignores it — GET takes no version-hold/lease. Human call
  whether truncating a concurrent read is acceptable S3 semantics for the first-deployment
  floor, or whether delete must honour the grace window it advertises.

- **NEEDS-HUMAN — "Real-SDK interop" is still not proven, and a stock aws-sdk streaming PUT
  is refused with 501.** Iteration-3 explicitly asked for a real-SDK interop path; the new
  round-trip test still signs with the gateway's *own* `sigv4::sign`
  (`crates/server/tests/s3_http_wire.rs:87`), so the only independent oracle is the AWS
  known-answer vectors (`crates/server/src/s3/sigv4.rs:558`,`588`) — which pin the signature
  *math* but not the wire plumbing (axum path/query extraction, header casing, chunked
  framing). Meanwhile a default modern aws-sdk / boto3 `put_object` that emits
  `x-amz-content-sha256: STREAMING-AWS4-HMAC-SHA256-PAYLOAD` is classified
  `PayloadHash::Streaming` (`crates/server/src/s3/sigv4.rs:372`) and rejected `501
  NotImplemented` (`crates/server/src/s3/mod.rs:218`–`226`). So "S3-compatible object store"
  (blueprint:382) holds only for clients configured to send a single non-chunked
  signed/UNSIGNED-PAYLOAD body. This is the pre-declared SigV4-scope NEEDS-HUMAN, but the
  brief/reviewer claim that the green "is not self-referential" rests entirely on KAT
  vectors — no actual SDK is exercised, so the wire-framing half of interop is unverified.

- **The recorded gating gate C4-ci (fail, exit 101) is NOT reproducible on the target and is
  not attributable to this diff.** I re-ran the exact gate command
  (`cargo test --workspace --exclude wyrd-dst`) → **exit 0, 83 test binaries green, 0
  failures**. I then stress-ran the diff's new concurrency tests to rule them out as the
  flake source: `s3_http_wire` 5× green, `concurrent_delete_is_idempotent` 15× green,
  `gc_delete_backstop` 3× green. `cargo build --workspace --exclude wyrd-dst` is also clean
  (exit 0), so it is not a compile red. This matches the iteration-3 adversary's read: the
  red is a stale/environmental flake, most likely in a *pre-existing, untouched* suite
  (`crates/server/tests/gateway_lease_expiry.rs` / `gateway_cluster.rs` — not in this
  patch). Advisory: this does **not** refute the fix; but the deterministic gate is still
  recorded red, so a human must re-run it green (and localize the flake) before accept — the
  gate cannot be waved off on the strength of the per-fix `run-verify` green alone, which
  only exercises the new test.

## Attempted to refute and could not (reported as strength)

- **DELETE crash-leak backstop is genuinely real now.** `metadata::unlink` writes an orphan
  grace record for every placed fragment in the *same atomic commit* that unbinds the object
  (`crates/core/src/metadata.rs:404`–`419`), and GC scans `ORPHAN_PREFIX` and reclaims after
  the grace window (`crates/custodian/src/gc.rs:37`–`40` re-uses the shared `orphan_key`;
  reconcile reads the ledger). The iteration-3 "no orphan record / false GC-backstop"
  complaint is resolved. Key protocol is single-sourced so writer/reader can't drift.
- **Delete idempotency holds under real races** — 64-round multi-thread test, 15× re-runs, 0
  failures; the CAS-conflict → re-resolve → `Ok(false)` branch (`lib.rs:250`+) is correct.
- **Crypto provenance corrected** — `crates/server/src/s3/crypto.rs` is now a thin wrapper
  over RustCrypto `sha2`/`hmac` (MIT/Apache, already allow-listed), still pinned to
  FIPS-180-4 / RFC-4231 / AWS KAT vectors. (Nit, not a finding: the `constant_time_eq`
  doc-comment at `crypto.rs:77` claims "length-independent" but the impl short-circuits on
  `a.len() == b.len()`; harmless here since hex signatures are fixed length.)
- **Auth precedes body materialisation** — `sigv4::verify` runs before the body stream is
  touched (`mod.rs:180`), and the "1 GiB declared, 0 sent → prompt 403" test confirms it.
- **Streaming is behaviourally demonstrated, not just present** — the `RecordingChunkStore`
  test asserts the first fragment lands after ≤2 pieces pulled of 16
  (`s3_http_wire.rs:501`), which genuinely fails for a buffering implementation.
- **XML error injection is closed** — `error_response` escapes both code and message
  (`mod.rs:322`–`323`), including the attacker-influenced streaming `sentinel`.
- **percent-decode has no off-by-one** — `i + 2 < bytes.len()` (`mod.rs:287`,
  `sigv4.rs:145`) is exactly the condition for a full trailing `%XX`; a truncated escape
  passes through literally, as documented.
- **`amz_date[..8]` (`sigv4.rs:339`) is not a panic vector** — `HeaderValue::to_str` only
  yields visible-ASCII (single-byte chars), so byte index 8 is always a char boundary.

### Advisory — codex

- NEEDS-HUMAN — `crates/server/tests/s3_http_wire.rs:80` still builds all signed HTTP requests with the gateway's own `sigv4::sign` helper, while the test-level interop rationale relies on unit vectors rather than a real `aws-sdk`/`boto3` request path. The prior carry-forward explicitly asked for a real-SDK interop path, so a human should decide whether this proof is sufficient for the S3 compatibility floor.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Four pre-declared human-only calls remain owed and are the crux of sign-off, not restated code: **(a) crypto provenance** — the hand-rolled MAC is replaced with RustCrypto sha2/hmac claiming an ADR-0003 three-test audit + deny.toml allowlist (patch.diff:174-191, 1221-1235); the human must confirm the audit was actually run and `cargo deny` is green (part of the red C4-ci). **(b) crate boundary** — builder *committed* to `crates/server` over the named `gateway-s3` crate (patch.diff:1391-1399); ratify or reverse. **(c) SigV4 scope / error-code floor** — header-only, presigned + aws-chunked out (501), a minimal error-code set (brief.md:136-148); confirm the floor. **(d) real-SDK interop still unproven** — the round-trip signs with the gateway's own `sign()` (patch.diff:2640); AWS published known-answer vectors de-risk canonicalization but no actual boto3/aws-sdk client drives the listener (iteration-3 open item, brief.md:173). Also: sequencing (M4 branch vs own sequence, brief.md:143-145) and prior-art-by-affected-path beyond the recorded v1-v3 iterations cannot be settled mechanically here.
- [ ] Validation — fitness-to-purpose — Human owns fitness of the Check floor to the first-deployment gate. The BINDING conditions are met in code and tested over loopback, but the LIVE "gateway serving real S3 to a public client over TLS" is pre-declared DEFERRED to #367 (brief.md:57-62,157-160); TLS is modeled (`TlsIdentity`) but unwired, an accepted deferral per iteration-2 carry-forward (brief.md:168). The human must judge whether the plaintext-loopback demonstrated floor is fit to gate #367 — and this cannot be accepted while the gating C4-ci gate is red.
- [ ] `crates/server/tests/s3_http_wire.rs:80` still builds all signed HTTP requests with the gateway's own `sigv4::sign` helper, while the test-level interop rationale relies on unit vectors rather than a real `aws-sdk`/`boto3` request path. The prior carry-forward explicitly asked for a real-SDK interop path, so a human should decide whether this proof is sufficient for the S3 compatibility floor.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101

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
- Iteration delta (if iterating): Gating C4-ci is RED on the record (`cargo test --workspace --exclude wyrd-dst` exit 101), so no green gate exists to accept against. Diagnosis: the failure is a non-reproducible, wall-clock-sensitive flake in a PRE-EXISTING, untouched test (`crates/server/tests/gateway_lease_expiry.rs`, which reads the real wall clock and asserts now+ttl vs now-ttl) — NOT attributable to this diff. Both reviewers re-ran the workspace green (adversary: 83 binaries green; new tests 5x/15x/3x green); the bundle captured only "exit status: 101" with no failing-test name. Likely aggravated by cross-lane host-clock skew (see §10). The merits are strong: DELETE crash-leak backstop, delete idempotency under 64-round races, RustCrypto provenance, auth-before-body, and behavioral streaming all survived adversarial attack. Re-run C4-ci to an authoritative GREEN gate and localize/quarantine the wall-clock flake so the record matches. Also address before re-accept (human calls owed at next Check): - Real-SDK interop still unproven: the round-trip signs with the gateway's own sigv4::sign; only AWS KAT vectors are the independent oracle (pins signature math, not wire framing). A stock modern aws-sdk streaming PUT (STREAMING-AWS4-HMAC-SHA256-PAYLOAD) is rejected 501 — so "S3-compatible" holds only for single non-chunked signed bodies. Add a real boto3/aws-sdk path or ratify the scope explicitly. - Concurrent GET-during-DELETE truncation: happy-path DELETE reclaims fragments eagerly, ignoring the orphan grace window the code advertises; a slow multi-chunk GET can be truncated mid-object by a concurrent DELETE. Decide whether that is acceptable first-deployment S3 semantics or DELETE must honour the grace window / GET must take a version-hold.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_364: cross-lane isolation gap — a host-clock-skew fault in one lane (e.g. #257's `date -s` ClockSkew nemesis on a network_mode:host container) can move the host wall clock and spuriously fail wall-clock-dependent gate tests in a sibling lane (e.g. `gateway_lease_expiry.rs`); consider isolating clock-skew faults or quarantining wall-clock tests from the gate.
