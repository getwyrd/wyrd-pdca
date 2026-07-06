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

# Check review — issue #364 / s3-http-wire-surface (iteration 5)

**Task under review:** give the Wyrd gateway its first client-facing network endpoint — an
S3-compatible HTTP listener serving **bucket-scoped object PUT / GET / DELETE** with mandatory
**SigV4** auth and **streaming** bodies, mapping onto the existing in-process client path so the
blueprint's day-one **PUT→GET→DELETE byte-identical round-trip** (blueprint:698–699) runs over the
wire; unsigned/bad-signature requests refused (no anonymous access). Fifth iteration — the prior
four were rejected for (v1–v2) buffering-not-streaming, self-referential SigV4, non-idempotent
DELETE, hand-rolled crypto; (v3–v4) DELETE crash-leak / placement-unaware reclaim and a red
gating C4-ci gate.

## Grounding note
`$PDCA_TARGET` resolves to the applied worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1` (the `s3`
module is present on disk), so citations ground on target source. I **could not independently
re-run `cargo`/`xtask ci`** — the harness sandbox refused every `cargo` invocation (approval-gated),
so C4/T3 verdicts lean on the recorded gates (`check-gates.json`: overall **pass**, C4-ci **pass**,
C4-verify **pass**) plus source re-derivation, not a fresh green from me. Flagged where it matters.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Binding floor is well-specified and the surface matches it: bucket-scoped PUT/GET/DELETE dispatch (`s3/mod.rs:201,219`), SigV4 verified fail-closed before body (`s3/mod.rs:187`), byte-identical streaming round-trip. Deferred live-TLS/#367 scope is pre-declared in the brief, not a spec gap. |
| C2 Reproduction (red pre-fix) | PASS | Net-new module ⇒ red is a compile-error red (brief allows this for net-new coverage); beyond that the assertions are behavioral (streaming pull-count `s3_http_wire.rs:554`, concurrent-DELETE `:355`, placement-aware delete `:840`, real-SDK chunked interop `:664`). Gate C4-verify records red→green. |
| C3 Change | PASS | Change reuses the client path rather than reimplementing it: HTTP verbs call `put_object_streaming`/`get_object_streaming`/`delete_object` (`s3/mod.rs:237,254,267`); wire layer stays generic over `Gateway<M,C,Co>` (ADR-0010), concretes only at the composition root. Delete seam widened into core `metadata::unlink` + custodian GC — in-scope per the brief (DELETE is net-new). |
| C4 Verification (red→green) | PASS | Recorded authoritative: `check-gates.json` C4-ci **pass** ("xtask ci: all checks passed"), C4-verify **pass**, overall **pass** — the gating red that killed v3/v4 is green on the record. NOT independently re-run here (sandbox blocked cargo); see §6 item for the human to confirm the historical `gateway_lease_expiry.rs` wall-clock flake is quarantined so the green is durable, not a lucky pass. Not a FAIL — the record is green and the failure was never attributable to this diff. |
| C5 Causal adequacy | NEEDS-HUMAN | Happy-path DELETE reclaims fragments **eagerly** (`lib.rs:245`→`reclaim_fragments` `lib.rs:285` `delete_fragment_at`), **bypassing the reader-safe grace window** the whole GC design rests on (`gc.rs:136`, 0005:291-294 "never tear an in-flight reader"). A slow multi-chunk streaming GET (`get_object_streaming` spawns a bounded-channel reader, `lib.rs:312-334`) can be truncated mid-object by a concurrent DELETE. Decision owed: is eager reclaim acceptable first-deployment S3 semantics, or must DELETE honour the grace window / GET take a version-hold? (Carried unresolved from iteration-v4.) The crash-leak root cause **is** genuinely fixed — orphan grace records written in the same atomic `unlink` batch (`metadata.rs:405`), GC reads+reclaims them (`gc.rs:113,152`). No capability-probe/runtime-guard smell (crypto is a real dep, TLS `Option` is deferred feature, `STREAMING-` dispatch is protocol not a probe). |
| T1 Structure | PASS | Wire surface lands **inside `crates/server`** (committed, documented `s3/mod.rs:25-33`, resolving the v2 crate-boundary re-deferral); `#![forbid(unsafe_code)]` held (HashingSource is `Unpin`, no pin-project). Auth/streaming/crypto split into focused modules. |
| T2 Shape | PASS | Tests colocated (`crates/server/tests/s3_http_wire.rs`), driven over a real loopback listener; independent oracles present — AWS published SigV4 KAT vectors (`sigv4.rs:629,659`) and AWS published aws-chunked streaming KAT (`streaming.rs:278`), so interop is AWS-correct not self-referential. |
| T3 Runtime | PASS | Per gate record the workspace test suite is green (C4-ci pass). Not independently re-run (sandbox); same caveat as C4. Streaming bounds peak memory to O(chunk) via a size-4 channel (`lib.rs:321`) and auth precedes body allocation (`s3/mod.rs:187`, tested `s3_http_wire.rs:261`). |
| T4 Contribution | PASS | Closes the load-bearing gap gating #367: the gateway gains its first network endpoint + auth boundary. Retires the "HTTP/S3 wire surface is a later milestone" marker; DELETE mapping is net-new as the brief required. |
| T5 Judgment | NEEDS-HUMAN | Two prior governance blockers now **resolved** and should be credited: crypto provenance moved to vetted RustCrypto `sha2`/`hmac` (`crypto.rs:17-18`, T5-a) and crate boundary committed (T5-b). Remaining human calls: (a) **SigV4 scope / error-code floor** — which signing variants + minimal S3 error-code subset the floor must guarantee (header-only accepted; presigned out of scope) is pre-declared human; (b) **sequencing** — M4 integration branch vs its own M4↔M7 sequence; (c) the DELETE grace-window semantics from C5. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Whether this floor is fit to serve the first-deployment gate is the human's call. **TLS remains plaintext loopback at Check** (`s3/mod.rs:45-52`, `TlsIdentity` modelled but unbound — a deny.toml/rustls-provider license decision, INTEGRATION.md §4); the live "gateway serving real S3 publicly over TLS" green is pre-declared off-Check at #367. Replay-within-15-min and UNSIGNED-PAYLOAD-on-plaintext are the accepted residuals tied to that TLS deferral. Human confirms the loopback-signed round-trip + AWS-KAT-pinned SigV4 is an acceptable stand-in for the deferred public-TLS deliverable. |

## §6 — items the human must clear
1. **C4/T3 gate not independently re-confirmed by the reviewer.** The record is green, but the
   sandbox blocked me from re-running `cargo test --workspace`/`xtask ci`. Given four prior
   iterations died on a non-reproducible wall-clock flake in the untouched
   `crates/server/tests/gateway_lease_expiry.rs`, confirm the gate is authoritatively green and the
   flake quarantined before accept.
2. **C5 — DELETE bypasses the reader-safe grace window (GET-during-DELETE tear).** Decide whether
   eager happy-path fragment reclaim (`lib.rs:245`) is acceptable first-deployment semantics or
   DELETE must honour the grace window / GET must take a version-hold. Carried from iteration-v4.
3. **T5 — SigV4 scope + error-code floor** ratification (header-only signing; minimal error-code
   subset). Pre-declared human call.
4. **T5 — sequencing**: M4 integration branch vs own M4↔M7 sequence. Pre-declared human call.
5. **Validation/TLS** — accept the plaintext-loopback-at-Check posture with public TLS deferred to
   #367, and clear the rustls-provider dependency/license decision when TLS is wired.

### Advisory — adversary

# Adversarial review — issue 364 / s3-http-wire-surface (iteration 5)

Advisory only; I never gate. Every `path:line` is grounded on the target source at
`$PDCA_TARGET` (= `/home/eddie/wyrd/wyrd.pdca-wt-l1`, patch applied). Scope: this diff.

## Findings

- **NEEDS-HUMAN — PUT overwrite leaks the prior object's fragments *permanently* — the very
  leak the DELETE path was rebuilt to close, now wide open on the more common verb.**
  `crates/server/src/lib.rs:182-202` (`commit_written`) routes an overwrite of an existing key
  to `write::commit_overwrite` (`crates/core/src/write.rs:265-272`), which only CAS-swaps the
  new chunk map onto the inode (`metadata::commit_chunk_map`). Unlike `delete_object`
  (`lib.rs:232-294`), the overwrite path **never** calls `reclaim_fragments` and **never**
  writes an orphan grace record — orphan records are written *only* in `metadata::unlink`
  (`crates/core/src/metadata.rs:394-411`). The old inode's committed fragments therefore carry
  no `pending:` lease and no `orphan:` record, so the custodian GC (which scans only those two
  key spaces) can never reclaim them. Concrete case: `PUT /b/k` (object A) then `PUT /b/k`
  (object B) — after the second commit, A's fragment bytes are stranded on every D-server
  forever, on the happy path, no crash required. This is *worse* than the crash-only DELETE
  leak the last three iterations fixed, and there is **no test** for it (the suite tests DELETE
  reclaim exhaustively — `crates/server/tests/s3_http_wire.rs:372,816` — but no overwrite-reclaim
  test exists). The reviewer's "does not leak fragment bytes" narrative (lib.rs:220-231) is
  scoped to DELETE and silently untrue for overwrite.

- **NEEDS-HUMAN — GET-during-DELETE truncates a streaming read; the docstring's "reader-safe
  grace window" is not honored on the happy path.** `delete_object`'s eager reclaim
  (`crates/server/src/lib.rs:245`, `reclaim_fragments` → `delete_fragment_at` at
  `lib.rs:285`) deletes the object's fragments **immediately**, not after any grace window.
  A concurrent `get_object_streaming` (`lib.rs:312-334`) resolves the chunk map up front and
  then reads fragments lazily on a spawned task (`lib.rs:324-332`); if the DELETE lands
  mid-stream, `read::read_chunk_verified` raises `MissingFragment`
  (`crates/core/src/read.rs:145,172`) and the reader task breaks, sending the client a
  truncated body. The GET response sets **no `Content-Length`** (`s3/mod.rs:255-259`) and has
  already emitted `200 OK`, so the client cannot cleanly distinguish truncation from success.
  For a single-chunk object the window truncates to zero bytes. This directly undercuts the
  binding "byte-identical round-trip" criterion under concurrent access. The docstrings claim
  a "reader-safe grace window" (`lib.rs:230-231`, `metadata.rs:356-358`) but that window is
  honored only by the crash/GC backstop — the happy-path reclaim ignores it. This is the exact
  concern iteration 4 left as an open "decide" item; it appears unaddressed.

- **NEEDS-HUMAN — "Real-SDK / stock-SDK interop" is still asserted, not demonstrated
  end-to-end.** The over-the-wire round-trip and streaming tests sign with the gateway's *own*
  `sigv4::sign` / `sign_with_payload_hash` and frame with the gateway's own helper
  (`crates/server/tests/s3_http_wire.rs:598,633`, comment at `:21`) — no real boto3/aws-sdk
  process ever hits the listener. The only independent oracles are unit KATs
  (`s3/sigv4.rs:629,659`; `s3/streaming.rs:278`), which pin the signing *math* but not the wire
  framing/header set a live SDK emits. So "a stock SDK upload round-trips instead of 501-ing"
  (mod.rs:19-23, streaming.rs:1-5) is verified only against the gateway's model of an SDK, not
  an SDK. This is the recurring carry-forward (iterations 2-4); it should be ratified as scoped
  or backed by a genuine SDK path, not accepted as "proven."

- Minor (fail-closed, not a hole): `verify` trims but does not collapse sequential internal
  whitespace in signed header values (`s3/sigv4.rs:403`), and re-sorts `SignedHeaders`
  (`sigv4.rs:384-385,405`). A real client that signs a header carrying doubled internal spaces,
  or sends `SignedHeaders` in a non-sorted order, would 403 — a spurious reject that further
  erodes the "real SDK compatibility" claim, though it never *weakens* auth.

## Attempted but could not refute

- **Percent-decode off-by-one** (`s3/mod.rs:298`, `s3/sigv4.rs:171`): the `i + 2 < len` guard
  is correct — a valid `%XX` at the very end of the segment still decodes. No boundary bug.
- **SigV4 canonicalization / signature math**: attacked query sorting, URI-encoding, and the
  signing-key ladder; the AWS `get-vanilla`, docs query-sort, and published streaming KATs
  (`sigv4.rs:629,659`; `streaming.rs:278`) are genuine independent oracles. Could not break the
  signing chain.
- **XML error injection** (`s3/mod.rs:317-344`): `xml_escape` covers all five predefined
  entities and is applied to both `<Code>` and `<Message>`. Could not inject markup.
- **DELETE idempotency under the CAS race** (`lib.rs:232-262`): the retry + re-resolve loop
  makes two concurrent DELETEs both succeed and bounds a pathological overwrite storm. Could
  not force a non-idempotent 409.
- **Auth-before-body / pre-auth amplification** (`s3/mod.rs:187-217`): `sigv4::verify` runs and
  must succeed before `body.into_data_stream()` is touched. Could not force a body allocation on
  an unsigned request.

### Advisory — codex

- NEEDS-HUMAN — [crates/server/src/lib.rs:245](/home/eddie/wyrd/wyrd.pdca-wt-l1/crates/server/src/lib.rs:245) still eagerly deletes the removed object's fragments on the successful DELETE path, and [crates/server/src/lib.rs:286](/home/eddie/wyrd/wyrd.pdca-wt-l1/crates/server/src/lib.rs:286) clears the orphan grace records immediately. That fixes the crash-leak backstop, but it bypasses the reader-safe grace window documented for orphan reclamation, so a slow GET that resolved the old inode before DELETE can lose fragments mid-stream once DELETE wins. Human needs to decide whether first-deployment S3 semantics allow truncating an already-started GET, or whether DELETE should leave fragments under the orphan ledger until GC's grace window expires / readers hold a version.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Happy-path DELETE reclaims fragments **eagerly** (`lib.rs:245`→`reclaim_fragments` `lib.rs:285` `delete_fragment_at`), **bypassing the reader-safe grace window** the whole GC design rests on (`gc.rs:136`, 0005:291-294 "never tear an in-flight reader"). A slow multi-chunk streaming GET (`get_object_streaming` spawns a bounded-channel reader, `lib.rs:312-334`) can be truncated mid-object by a concurrent DELETE. Decision owed: is eager reclaim acceptable first-deployment S3 semantics, or must DELETE honour the grace window / GET take a version-hold? (Carried unresolved from iteration-v4.) The crash-leak root cause **is** genuinely fixed — orphan grace records written in the same atomic `unlink` batch (`metadata.rs:405`), GC reads+reclaims them (`gc.rs:113,152`). No capability-probe/runtime-guard smell (crypto is a real dep, TLS `Option` is deferred feature, `STREAMING-` dispatch is protocol not a probe).
- [ ] T5 Judgment — Two prior governance blockers now **resolved** and should be credited: crypto provenance moved to vetted RustCrypto `sha2`/`hmac` (`crypto.rs:17-18`, T5-a) and crate boundary committed (T5-b). Remaining human calls: (a) **SigV4 scope / error-code floor** — which signing variants + minimal S3 error-code subset the floor must guarantee (header-only accepted; presigned out of scope) is pre-declared human; (b) **sequencing** — M4 integration branch vs its own M4↔M7 sequence; (c) the DELETE grace-window semantics from C5.
- [ ] Validation — fitness-to-purpose — Whether this floor is fit to serve the first-deployment gate is the human's call. **TLS remains plaintext loopback at Check** (`s3/mod.rs:45-52`, `TlsIdentity` modelled but unbound — a deny.toml/rustls-provider license decision, INTEGRATION.md §4); the live "gateway serving real S3 publicly over TLS" green is pre-declared off-Check at #367. Replay-within-15-min and UNSIGNED-PAYLOAD-on-plaintext are the accepted residuals tied to that TLS deferral. Human confirms the loopback-signed round-trip + AWS-KAT-pinned SigV4 is an acceptable stand-in for the deferred public-TLS deliverable.
- [ ] [crates/server/src/lib.rs:245](/home/eddie/wyrd/wyrd.pdca-wt-l1/crates/server/src/lib.rs:245) still eagerly deletes the removed object's fragments on the successful DELETE path, and [crates/server/src/lib.rs:286](/home/eddie/wyrd/wyrd.pdca-wt-l1/crates/server/src/lib.rs:286) clears the orphan grace records immediately. That fixes the crash-leak backstop, but it bypasses the reader-safe grace window documented for orphan reclamation, so a slow GET that resolved the old inode before DELETE can lose fragments mid-stream once DELETE wins. Human needs to decide whether first-deployment S3 semantics allow truncating an already-started GET, or whether DELETE should leave fragments under the orphan ledger until GC's grace window expires / readers hold a version.

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
- Iteration delta (if iterating): Rejected (iter-5): gates are green and prior blockers are genuinely fixed (RustCrypto sha2/hmac, crate boundary committed, SigV4 AWS-KAT-pinned, DELETE crash-leak root cause closed) — but the adversary found a new permanent leak on the happy path plus two carried-forward gaps that must be closed before accept. (1) BLOCKING — PUT overwrite leaks the prior object's fragments permanently. `commit_written` (`crates/server/src/lib.rs:182-202`) routes an overwrite of an existing key to `write::commit_overwrite` (`crates/core/src/write.rs:265-272`), which only CAS-swaps the new chunk map onto the inode and NEVER calls `reclaim_fragments` and NEVER writes an orphan grace record (orphan records exist only in `metadata::unlink`, `crates/core/src/metadata.rs:394-411`). After `PUT /b/k` (A) then `PUT /b/k` (B), A's fragment bytes are stranded on every D-server forever, on the happy path, no crash. This is the SAME leak class the last three iterations fixed for DELETE, now reopened on the more common verb — and worse (no crash required). There is NO test. Fix: route overwrite through the same orphan-grace-record + GC-reclaim path DELETE uses, and add an overwrite-reclaim regression test (mirror the DELETE reclaim tests at `crates/server/tests/s3_http_wire.rs:372,816`). (2) BLOCKING — GET-during-DELETE truncates a streaming read (codex + C5, carried UNADDRESSED from iter-4). `delete_object`'s eager reclaim (`lib.rs:245` → `reclaim_fragments` → `delete_fragment_at` `lib.rs:285`) deletes fragments immediately, and `lib.rs:286` clears the orphan grace records immediately, bypassing the reader-safe grace window the GC design rests on (`gc.rs:136`, 0005:291-294). A concurrent `get_object_streaming` (`lib.rs:312-334`) resolves the chunk map up front then reads lazily; if DELETE lands mid-stream, `read_chunk_verified` raises `MissingFragment` (`crates/core/src/read.rs:145,172`) and the reader sends a truncated body — no `Content-Length` set and `200 OK` already emitted (`s3/mod.rs:255-259`), so the client cannot distinguish truncation from success. Single-chunk objects truncate to zero bytes. This directly undercuts the binding byte-identical round-trip under concurrent access. Make DELETE honour the grace window (leave fragments under the orphan ledger until GC's grace expires) or have GET take a version-hold; add a GET-during-DELETE regression. (3) BLOCKING — real-SDK interop is asserted, not demonstrated. The over-the-wire round-trip/streaming tests sign with the gateway's OWN `sigv4::sign` / `sign_with_payload_hash` and frame with the gateway's own helper (`crates/server/tests/s3_http_wire.rs:598,633`, comment :21) — no real boto3/aws-sdk process ever hits the listener. The unit KATs (`s3/sigv4.rs:629,659`, `s3/streaming.rs:278`) pin the signing math but not the wire framing/header set a live SDK emits. The "stock SDK round-trips instead of 501-ing" claim (mod.rs:19-23) is verified only against the gateway's model of an SDK. Recurring carry-forward from iterations 2-4. REQUIRED at-Check path (maintainer decision at sign-off): add the real Rust `aws-sdk-s3` (+ `aws-config`) as a dev-dependency of `crates/server`, point its endpoint at the loopback listener with path-style addressing and static creds, and drive `put_object` → `get_object` → `delete_object` asserting byte-identity — a genuine independent oracle whose signer/framer is NOT `crates/server`'s own `sigv4`/streaming helpers (that self-reference is exactly what was refuted). This runs under plain `cargo test`, no container, deterministic. Before committing, check the `aws-sdk-s3` dependency tree against `deny.toml` / INTEGRATION.md §4 (license/allowlist) — flag as a NEEDS-HUMAN if it introduces a denied crate. A live boto3/aws-cli Tier-2 leg stays a pre-declared DEFERRED backstop, not the at-Check bar. While here, fix the minor fail-closed SDK-compat erosions the adversary flagged: `verify` does not collapse sequential internal whitespace in signed header values (`s3/sigv4.rs:403`) and re-sorts `SignedHeaders` (`sigv4.rs:384-385,405`), so a real client signing doubled internal spaces or sending non-sorted SignedHeaders gets a spurious 403. Not re-litigated (standing human calls owed at the next Check, not blocking the rebuild): T5 SigV4 scope / minimal S3 error-code floor; M4 sequencing; Validation/TLS plaintext-loopback-at-Check posture with public TLS deferred to #367 (rustls-provider deny.toml/license decision when TLS is wired). Also confirm at the next Check that the historical `gateway_lease_expiry.rs` wall-clock flake is quarantined so the gate green is durable (reviewer could not re-run cargo here).
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
