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

# Check review — issue 364 / s3-http-wire-surface (iteration 7)

**Task under review:** give the in-process gateway a real client-facing **HTTP/S3 wire
surface** — bucket-scoped object PUT/GET/DELETE over S3-compatible HTTP with mandatory
SigV4 auth and streaming bodies — so the blueprint's day-one byte-identical S3 round-trip
can run over the network (brief.md:44-56). This iteration answers iter-6's reject:
(1) extract the wire layer to a dedicated crate, (2) close the streaming fail-open,
(3) resolve trailer-framing half-accept, (4) quarantine the lease-expiry wall-clock flake.

**Grounding:** target `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`, readable and
patch-applied (gateway-s3/gateway-core crates present; lease-expiry SKEW slack present).
The target is **not** stale. Cargo/git/test-binary execution is blocked by the sandbox, so
I could **not** re-run the gate or the test binaries myself; runtime rows lean on the
recorded gate plus static reading of the applied source.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief pins a demonstrable floor — signed bucket-scoped PUT→GET→DELETE byte-identical + unsigned/bad-sig refused, streaming, fail-closed (brief.md:48-56); the patch targets exactly this. Spec is unambiguous. |
| C2 Reproduction (red pre-fix) | PASS | Net-new module: red is the compile/absence red, made behavioral per brief.md:108. Non-gating C4-verify recorded PASS ("red without the fix, green with it", check-gates.json:46); I could not re-run it (sandbox). |
| C3 Change | PASS | Change delivers the endpoint: `gateway-s3` listener + SigV4 verify (`crates/gateway-s3/src/sigv4.rs:472-490`), aws-chunked streaming decode (`streaming.rs:114-252`), verbs mapped onto the client seam (`crates/server/src/lib.rs:131,199,279`). Grounds on target. |
| C4 Verification (red→green) | **FAIL** | **Gating** C4-ci `cargo test --workspace --exclude wyrd-dst` recorded **exit 101** (check-gates.json:33-39) — a runtime test failure on an applied, compiling patch (test binaries built in target/), not a stale-apply artifact. Decision owed: this is the blocker; it must be re-run to an authoritative GREEN before accept. Sandbox prevented me from re-running/localizing. |
| C5 Causal adequacy | PASS | Root cause (no wire endpoint) is removed, not guarded. Prior leak causes stay closed: overwrite orphans the superseded map atomically (`crates/core/src/write.rs:271-287` → `crates/core/src/metadata.rs:459-485`); DELETE honours the grace window, no eager reclaim (`crates/server/src/lib.rs:290-294`). Symptom-guard smell-test: no capability-probe / load-time runtime guard introduced. Orphan-ledger design was ratified in iter-6 — not re-litigated. |
| T1 Structure | PASS | iter-6 T5(a) reject answered: wire layer extracted to `crates/gateway-s3` over a shared `crates/gateway-core` seam (new crates present in target), composition-root wiring preserved — it no longer calcifies inside `crates/server`. |
| T2 Shape | PASS | Fail-closed shape: closed-set sentinel classification refuses unknown STREAMING-* cleanly (`sigv4.rs:472-490,506-514`), declared chunk size bounded before buffering (`streaming.rs:57,215-219`), XML error `<Message>` escaped (`gateway-s3/src/lib.rs:324-345`). All named iter-6 fail-open/half-accept holes addressed. |
| T3 Runtime | NEEDS-HUMAN | Decision owed: is the recorded exit-101 the historical `gateway_lease_expiry.rs` wall-clock flake (this iter slackens the lower bound 20s, `crates/server/tests/gateway_lease_expiry.rs:148-152`) or a real regression in the new real-SDK/concurrency tests (`s3_http_wire.rs:331,382,792`)? Impact: it decides whether C4 is a durable green or a live defect. Sandbox blocked my re-run; human must localize on a clean host. |
| T4 Contribution | PASS | Contributes the load-bearing wire seam gating #367; core/custodian edits (overwrite-reclaim, GC backstop) are the in-scope leak fixes prior iterations demanded, not scope creep. Real `aws-sdk-s3` dev-dep is the independent oracle (`crates/server/tests/s3_http_wire.rs:792`, Cargo.toml:82), retiring the self-signing objection. |
| T5 Judgment | NEEDS-HUMAN | Ratified in iter-6 (crate boundary, SigV4 header-only scope, M4 sequencing, TLS deferral, aws-sdk-s3 deny-clean) — do not re-litigate. Residual open call: the **minimal S3 error-code floor** the gateway must return correctly (auth-failure / not-found subset — brief Known-NEEDS-HUMAN item 4, brief.md:146-148) is not yet ratified. Human must fix the floor to avoid the S3-conformance rabbit hole. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: does a **plaintext-loopback** signed PUT→GET→DELETE round-trip with backend-run (not at-Check) fragment reclamation constitute an acceptable first-deployment-gate floor, with live public-TLS + running custodian topology deferred to #367 (brief.md:57-62,150-160)? This is the human's fitness call — and it cannot be signed while the gating C4-ci above is red. |

### Advisory — adversary

# Adversarial review — issue 364 / s3-http-wire-surface (iteration 7)

Lens: refute the red→green evidence and the reviewer's verdict; find the input that breaks the fix.
Inputs used: `patch.diff`, `brief.md`, `check-gates.json`; every `path:line` grounded on the
target source at `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`). Advisory only — no gate.

## Evidence I attacked and could NOT refute (reported honestly)

- **The recorded gating gate (C4-ci) does not reproduce as a defect.** `check-gates.json`
  records `C4 Wyrd gate … cargo test --workspace --exclude wyrd-dst failed with exit status:
  101` (gating, fail). I re-ran it on the target and it passed clean (`EXIT=0`, no `FAILED`
  lines across ~60 binaries). `cargo deny check` → `advisories/bans/licenses/sources ok`;
  `cargo clippy -p wyrd-gateway-s3 -p wyrd-gateway-core -p wyrd-server --all-targets` → no
  warnings/errors. This matches the iter-3/4 diagnosis that the red is the historical
  wall-clock flake, not this diff. The patch's quarantine of that flake
  (`crates/server/tests/gateway_lease_expiry.rs:135-158`, +`SKEW_ALLOWANCE_MILLIS`) held over
  3 repeats; `s3_http_wire` (12 tests) green over 2 repeats. **I could not turn the recorded
  red into a real failure.**
- **Attempted to break overwrite-reclaim via empty (identity) placement — does not hold.**
  `commit_chunk_map_superseding` (`crates/core/src/metadata.rs:459`) writes orphan records
  over `prior.chunk_map … chunk.fragments()`; I suspected a pre-M3 empty `placement` vector
  would yield zero fragments and leak the prior object. It doesn't: `fragments()`
  (`metadata.rs:175`) expands the full `0..fragment_count()` index space through the identity
  fallback even for an empty vector, matching GC's `referenced_fragments`. The overwrite-
  reclaim integration test (`crates/custodian/tests/gc_delete_backstop.rs:196`) drives the real
  `commit_chunk_map_superseding` + real `reconcile_step`, so it exercises the production reclaim
  path, not a mirror.
- **Attempted the iter-5 SigV4 fail-closed erosions — fixed.** `verify` now Trimall-collapses
  internal whitespace (`crates/gateway-s3/src/sigv4.rs:433` `trim_all`) and uses the client's
  `SignedHeaders` verbatim in the string-to-sign (`sigv4.rs:439`) rather than re-sorting — so a
  client signing doubled spaces / non-sorted SignedHeaders no longer gets a spurious 403.
- **Attempted the iter-6 streaming fail-open — fixed.** The declared chunk size is bounded
  before any body is buffered (`crates/gateway-s3/src/streaming.rs:215`, `MAX_CHUNK_SIZE`),
  malformed headers → framing error (400), and `-TRAILER` sentinels are a closed accept-set
  (`sigv4.rs:508-511`), not a `starts_with("STREAMING-")` half-accept.
- **Real-SDK oracle is now genuine.** `real_aws_sdk_put_get_delete_round_trips_byte_identical`
  (`crates/server/tests/s3_http_wire.rs:791`) drives the real `aws-sdk-s3` (its own
  signer/canonicalizer/aws-chunked framer) at the loopback listener; nothing on that path calls
  the gateway's own `sigv4`/`streaming`. This closes the iter-2..5 self-reference refutation.

## Concrete residual findings a human should adjudicate

- **NEEDS-HUMAN — Streaming GET truncates silently under a mid-stream fragment fault.**
  `get_object_streaming` (`crates/server/src/lib.rs:238-256`) resolves the chunk map, then the
  spawned reader breaks the loop on the *first* `read_chunk_verified` error
  (`lib.rs:250-253`) — but the HTTP handler has already emitted `200 OK` with no
  `Content-Length` and no `ETag` (`crates/gateway-s3/src/lib.rs:262-266`). Concrete failing
  case: GET of a ≥2-chunk object whose second fragment is unavailable/checksum-fails (a genuine
  D-server fault, not DELETE — which now defers to the grace window) → the client receives
  `200 OK` + a **partial** body and no S3 error code; a single-chunk fault truncates to zero
  bytes. The buffered `get_object` (`lib.rs:168`) surfaces the same fault as a failed GET. A
  correct HTTP/1.1 client can detect the missing terminating chunk as a transport error, but
  the gateway has still promised success and emitted partial object bytes. This is net-new to
  this diff's streaming-GET surface. Is silent-partial-200 acceptable first-deployment GET
  semantics, or must GET buffer-to-first-error / send a trailer error / set Content-Length?

- **NEEDS-HUMAN — the gating record and reality disagree; the record, not the re-run, blocks.**
  `check-gates.json` is `overall: fail` with `C4-ci` red, while `C4-verify` (per-fix
  red→green) is green and my independent full-workspace re-run is green. The deterministic gate
  is what governs sign-off, and it is red. The quarantine widens the lease-expiry bounds by 20s
  (`gateway_lease_expiry.rs:150`), which is a *plausible* absorption of NTP skew but is proven
  only by green runs, not by injecting a backward clock step. The record must be re-run to an
  authoritative green (durably, under the CI host's clock) before accept — do not lean on the
  per-fix `run-verify` green, which only exercises the new test.

## Minor / lower-confidence (not blocking, noted for completeness)

- The real-SDK round-trip (`s3_http_wire.rs:799`, 9000-byte object) is a black-box PUT→GET; it
  proves a stock SDK round-trips but cannot assert *which* wire form (single-shot `Signed` vs
  `STREAMING-UNSIGNED-PAYLOAD-TRAILER`) the SDK chose, so the signed-aws-chunked decode path's
  real-SDK exercise rests on the hand-framed `stock_sdk_chunked_put_round_trips_byte_identical`
  (`s3_http_wire.rs:673`), not the SDK itself. Adequate, but the "stock SDK exercises the signed
  streaming decoder" claim is one step weaker than stated.
- GET responses carry no `ETag` and no `Content-Length` (`gateway-s3/src/lib.rs:262-266`); the
  reviewed SDK tolerates it, but this is an S3-compat gap (some clients require them). Likely
  covered by the brief's "wire encoding is ILLUSTRATIVE" scope — flagging so it is a *decision*,
  not an omission.

### Advisory — codex

- `crates/server/src/cli.rs:743` composes the new long-lived S3 role over persistent redb/fs state with `Gateway::new`, but that constructor resets `next_inode`/`next_chunk` to `1` on every process start (`crates/server/src/lib.rs:85`). After a restart, new-key PUTs can spuriously conflict against existing inode ids, and overwrites can mint chunk ids that already back committed objects before the CAS publishes the new map. The S3 server role needs a durable allocator or startup recovery from metadata/chunk state.
- `crates/core/src/write.rs:436` computes one pending-lease expiry for the entire streaming PUT and stamps every later chunk with that same deadline; `crates/server/src/lib.rs:209` starts the stream with the gateway's fixed 30s TTL. A slow authenticated upload can run past that deadline before commit, letting a concurrent custodian sweep reclaim early chunks as expired pending garbage and then publish an object with missing fragments. Renew leases or stamp each chunk relative to its actual write time / hold the upload until commit.
- `crates/gateway-s3/src/sigv4.rs:508` accepts the signed and unsigned `...-TRAILER` streaming sentinels, but `crates/gateway-s3/src/streaming.rs:244` treats the zero-length chunk as completion and returns without parsing or validating trailing headers/checksums/signatures. That is still a half-accept for trailer framing; either reject trailer sentinels cleanly or implement trailer consumption and verification before committing the PUT.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T3 Runtime — Decision owed: is the recorded exit-101 the historical `gateway_lease_expiry.rs` wall-clock flake (this iter slackens the lower bound 20s, `crates/server/tests/gateway_lease_expiry.rs:148-152`) or a real regression in the new real-SDK/concurrency tests (`s3_http_wire.rs:331,382,792`)? Impact: it decides whether C4 is a durable green or a live defect. Sandbox blocked my re-run; human must localize on a clean host.  [CLEARED: clean-host re-run — full workspace EXIT=0, no FAILED; gateway_lease_expiry 5/5 green — confirms historical wall-clock flake, not a live regression.]
- [x] T5 Judgment — Ratified in iter-6 (crate boundary, SigV4 header-only scope, M4 sequencing, TLS deferral, aws-sdk-s3 deny-clean) — do not re-litigate. Residual open call: the **minimal S3 error-code floor** the gateway must return correctly (auth-failure / not-found subset — brief Known-NEEDS-HUMAN item 4, brief.md:146-148) is not yet ratified. Human must fix the floor to avoid the S3-conformance rabbit hole.  [RATIFIED: the floor — differentiated auth-failure codes (AccessDenied/AuthorizationHeaderMalformed/InvalidAccessKeyId/SignatureDoesNotMatch, sigv4.rs:108-113) + NoSuchKey/OperationAborted/XAmzContentSHA256Mismatch (lib.rs:268-385) — is sufficient to execute and assert a durability round-trip; the full S3 code sweep stays deferred to a pre-M8 gate.]
- [ ] Validation — fitness-to-purpose — Decision owed: does a **plaintext-loopback** signed PUT→GET→DELETE round-trip with backend-run (not at-Check) fragment reclamation constitute an acceptable first-deployment-gate floor, with live public-TLS + running custodian topology deferred to #367 (brief.md:57-62,150-160)? This is the human's fitness call — and it cannot be signed while the gating C4-ci above is red.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101  [CLEARED on same basis as T3: clean-host re-run of `cargo test --workspace --exclude wyrd-dst` EXIT=0, no FAILED; recorded exit-101 attributed to the gateway_lease_expiry wall-clock flake.]

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
- Iteration delta (if iterating): issue_364 — the feature floor is sound (signed loopback PUT->GET->DELETE, differentiated auth codes, streaming fail-closed), but two codex durability findings MUST be resolved before this ships — they sit exactly on the M4 production-metadata-backend durability seam: 1. Durable id allocator / startup recovery. `Gateway::new` resets `next_inode`/`next_chunk` to 1 on every process start (crates/server/src/lib.rs:85) over persistent redb/fs state. After a restart, new-key PUTs spuriously conflict against existing inode ids, and overwrites can mint chunk ids that already back committed objects before the CAS publishes the new map -> corruption/loss. Add a durable allocator or recover the high-water marks from metadata/chunk state at startup. 2. Per-chunk lease deadlines. The streaming PUT computes one 30s expiry and stamps it on every chunk (crates/core/src/write.rs:436; server lib.rs:209). A slow authenticated upload can run past that deadline before commit, letting a concurrent custodian sweep reclaim early chunks as expired pending garbage and publish an object with missing fragments. Stamp each chunk relative to its own write time / renew leases / hold reclaim until commit. Add durability tests that actually exercise these — a restart-then-GET/PUT round-trip and a slow-PUT-under-custodian-sweep. The current loopback suite exercises neither, which is why both defects passed green. Ratified this iteration — do NOT re-litigate or re-defer: - C4 gate red was the `gateway_lease_expiry` wall-clock flake, not a regression: clean-host `cargo test --workspace --exclude wyrd-dst` = EXIT 0, lease-expiry test 5/5 green (see §6.1/§6.4). - The minimal S3 error-code floor is sufficient for durability testing (see §6.2). Keep it; do not chase the full S3 conformance sweep. Also worth folding in (adversary, same fault-under-durability theme): streaming GET emits 200 OK + partial body on a mid-stream fragment fault (crates/server/src/lib.rs:238-256; gateway-s3/src/lib.rs:262-266) with no S3 error code — decide buffer-to-first-error / trailer-error / Content-Length while reworking the above.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_364: recurring gating-gate failure driven by the `gateway_lease_expiry` wall-clock flake — recorded C4-ci went red again while clean-host re-runs are green; look at quarantining/derandomizing the flake or gating on a repeat-run so the record stops diverging from reality.
