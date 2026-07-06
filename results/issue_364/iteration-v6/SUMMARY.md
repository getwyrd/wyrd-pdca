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

# Check review — issue 364 / s3-http-wire-surface (iteration 6)

**Task under review:** give the in-process gateway a real client-facing **S3-compatible HTTP
endpoint** — bucket-scoped object PUT/GET/DELETE, mandatory SigV4 signature verification,
streaming bodies — mapping onto the existing `Gateway::{put,get}_object` client paths, so the
blueprint's day-one signed round-trip can run over the wire (public TLS/live deploy deferred to
the first-deployment gate #367). This iteration must close the three iter-5 BLOCKERs:
(1) PUT-overwrite fragment leak, (2) GET-during-DELETE truncation, (3) real-SDK interop proven,
not asserted.

Grounded read-only against the target worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1` (patch applied;
s3 module + `s3_http_wire.rs` present and matching the diff). Cargo re-run was **approval-blocked
in this sandbox** (as in prior iterations) — verification leans on the recorded green gate plus
line-level source grounding of the tests and implementation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Binding criterion is met in code: signed bucket-scoped PUT/GET/DELETE over HTTP with fail-closed SigV4 (`s3/mod.rs`, `s3/sigv4.rs:413` verify), byte-identical round-trip, streaming bodies (`lib.rs:283` `get_object_streaming`). Public-TLS/live-deploy correctly held off-Check to #367 per brief §DEFERRED. |
| C2 Reproduction (red pre-fix) | PASS | Net-new coverage; the three iter-5 blockers now have **behavioral** reds, not just compile reds: overwrite-reclaim (`custodian/tests/gc_delete_backstop.rs:196`), GET-during-DELETE truncation (`s3_http_wire.rs:381`), real-SDK round-trip (`s3_http_wire.rs:791`). check-gates C4-verify records "red without the fix, green with it." |
| C3 Change | PASS | Overwrite routed through `write::commit_overwrite`→`metadata::commit_chunk_map_superseding` (`metadata.rs:459`); DELETE defers reclaim to the grace window (`lib.rs:235`); real `aws-sdk-s3` dev-dep added (`crates/server/Cargo.toml:81`); sigv4 Trimall + verbatim SignedHeaders fixes (`sigv4.rs:191,439`). Coherent, scoped to the wire surface + its leak counterparts. |
| C4 Verification (red→green) | PASS | Gate C4-ci and C4-verify both recorded **pass** (check-gates.json). Could not independently re-run cargo (approval-blocked). CAVEAT for the human: the pre-existing wall-clock flake `crates/server/tests/gateway_lease_expiry.rs:7-8` (reads real clock) is **still not quarantined** despite the iter-5 carry-forward asking for it — the green gate is real this run but its durability across host-clock skew is unconfirmed. |
| C5 Causal adequacy | NEEDS-HUMAN | In-scope root cause is genuinely closed: both DELETE **and** overwrite now write an orphan grace record per superseded fragment in the *same atomic commit* (`metadata.rs:483`,`:407`), and GC reclaims only after the reader-safe window with a never-reclaim-referenced safety gate (`custodian/gc.rs:113,121`). Smell-test does not fire (removes the cause, no capability probe). **Decision owed:** across 5 iterations each Check surfaced a *new* supersede-without-orphan leak class (DELETE→overwrite); confirm no remaining path strands bytes — notably rebalance/reconstruction re-placement, which `metadata.rs:454` states is **"deliberately left non-orphaning."** Whether that is safe or the next leak variant is a durability call the human should ratify. |
| T1 Structure | PASS | Wire layer is a self-contained `s3/{mod,sigv4,crypto,streaming}` generic over `Gateway<M,C,Co>`; concretes wired only at the composition root (`s3/mod.rs:2415-2416`), preserving ADR-0010 one-place wiring. |
| T2 Shape | PASS | API surface maps 1:1 onto S3 verbs and reuses the client seam (`put_object`/`get_object`/`get_object_streaming`/`delete_object`, `lib.rs`); no reimplementation of the write/read path. Streaming GET uses a bounded channel (`lib.rs:292`) so peak resident stays O(chunk), honouring the 0015:789 OOM invariant. |
| T3 Runtime | PASS | Fail-closed auth (unsigned/wrong-cred → 403/InvalidAccessKeyId, `s3_http_wire.rs:231,290,847`); auth-before-body; streaming demonstrated > chunk size. Gate green; runtime not independently re-run here (cargo approval-blocked). |
| T4 Contribution | PASS | Net-new load-bearing wire seam exercised at Check (signed round-trip succeeds, unsigned refused), not dead scaffolding. Prior art by affected path = the preserved iteration-v1..v5 history; no competing merged/rejected wire surface. |
| T5 Judgment | NEEDS-HUMAN | Resolved this round: crypto provenance (RustCrypto `sha2`/`hmac`, already deny-allowed), crate boundary committed to `crates/server` (Cargo.toml:3). **Decisions owed:** (a) ratify the crate-boundary choice vs the architecture-named `gateway-s3` crate; (b) SigV4 accepted-variant scope + the minimal S3 error-code floor (brief Known-NEEDS-HUMAN); (c) M4-branch vs own-sequence placement; (d) the new `aws-sdk-s3` dev-dep tree passed `cargo deny` with **no deny.toml change** (introduces no denied crate) but is a sizeable new dev-time supply-chain surface worth explicit awareness. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | **Decision owed at sign-off:** is the plaintext-loopback-at-Check posture (TLS modelled via `TlsIdentity` but unbound — `s3/mod.rs:2424-2431`) the accepted floor, with real public-TLS + live "gateway serving S3" green deferred to #367? Brief pre-declares this as off-Check, but fitness-for-first-deployment and the durability of the green gate (unquarantined lease-expiry flake, C4 caveat) are human calls the deterministic gates cannot make. |

### Advisory — adversary

# Adversarial review — issue 364 / s3-http-wire-surface (iteration 6)

Skeptic's pass. Attacked the red→green evidence, the fix, and the reviewer verdict.
Ground truth read on `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`. Advisory only —
no gate. Scope limited to this diff.

## Findings

- **NEEDS-HUMAN — The `aws-chunked` decoder does not bound the *declared* chunk size, re-opening
  the "stream, don't buffer" OOM cliff (0015:789) and, worse, turning a malformed size into a
  silent `200 OK` truncated write.** `crates/server/src/s3/streaming.rs:195` loops
  `while self.buf.len() < size + 2 { fill }` where `size` comes straight from the attacker's chunk
  header (`parse_chunk_header`, `streaming.rs:238`, `usize::from_str_radix` with **no upper bound**),
  and the chunk signature is only checked *after* the whole chunk is buffered
  (`streaming.rs:210`). Two concrete failing cases, both reachable by an **authenticated** client
  (only the seed signature must verify):
  - *Memory amplification:* a chunk header `40000000;chunk-signature=…` (1 GiB) followed by the
    bytes forces up to 1 GiB resident in `Decoder::buf` before anything drains; k concurrent PUTs →
    k GiB. This is exactly the per-request-cap OOM class iteration 1/2 flagged as inverting the
    "stream, don't buffer" invariant — now unbounded and untested (the streaming tests use tiny
    64 KiB/uneven chunks only).
  - *Overflow → silent truncation:* a chunk header `ffffffffffffffff;chunk-signature=…` makes
    `size + 2` (`streaming.rs:195`) overflow — panic in the debug/test build, out-of-bounds slice
    at `streaming.rs:200` otherwise. The panic occurs in the **spawned** decode task, which merely
    drops the channel sender; `ReceiverStream` then yields `None`, so `put_object_streaming` sees a
    clean EOF and **commits**. Because the streaming body is handed to the writer as
    `PayloadHash::Unsigned` (`crates/server/src/s3/mod.rs:237`), there is *no* end-to-end integrity
    check, so a malformed-framing request that should be `400 InvalidRequest` instead returns
    `200 OK` for a truncated/empty object. This is a fail-open on the write path, contradicting the
    fail-closed intent. No test exercises a large or malformed chunk-size header, so the green gate
    says nothing about it.

- **NEEDS-HUMAN — The DELETE/overwrite "GC reclaims it" backstop cannot run in the one runnable
  server role this patch ships.** The iteration-5 BLOCKING leaks were closed by deferring all
  reclaim to the custodian GC loop: `unlink` / `commit_chunk_map_superseding` write durable orphan
  records (`crates/core/src/metadata.rs`) and nothing reclaims eagerly. But the delivered gateway
  process, `cli::cmd_s3` (`crates/server/src/cli.rs:716`), composes `RedbMetadataStore::open(...)`
  + `MemCoordination::new()` (`cli.rs:745`) and **spawns no custodian** (confirmed by the standing
  comment `cli.rs:53` "The CLI runs no custodian sweep"). The custodian GC lives in a separate
  crate/process; with a single-writer, file-locked redb store owned by the gateway and in-memory
  (non-shared) coordination, no co-process custodian can attach to reclaim. So in the runnable
  `wyrd s3` role, orphaned fragments from every DELETE and every PUT-overwrite accumulate
  unreclaimed for the life of the process — the same leak class three prior iterations treated as
  blocking, now relocated behind a backstop no co-running process can service. The orphan records
  are durable (a later/tikv-shared custodian *could* reclaim), so this is a topology gap a human
  must adjudicate against the #367 deployment plan, not a proof of permanent loss.

- **The "real-SDK interop closes break 1 (percent-encoded key identity)" claim is only half
  demonstrated.** The genuine `aws-sdk-s3` round-trip
  (`crates/server/tests/s3_http_wire.rs`, `real_aws_sdk_put_get_delete_round_trips_byte_identical`)
  uses key `real-sdk/round-trip-object` — **no** space or `%`-requiring character. The specific
  carry-forward break ("boto3 key `my file.txt` stored/keyed as `my%20file.txt`") is still proven
  only by the unit test `percent_decode_recovers_the_true_key` and the gateway's own self-signed
  wire tests (`s3/mod.rs` `percent_decode_utf8`), i.e. self-referentially. No real SDK request with
  an encoding-sensitive key ever hits the listener, so the interop oracle does not actually cover
  the byte the break was about. Low blast radius (S3 single-encodes the S3 canonical URI, so it
  likely holds), but the reviewer's "not self-referential" verdict overstates coverage for this
  case.

## Attempted and could not refute

- DELETE idempotency under a concurrent race: `delete_object` (`crates/server/src/lib.rs`) resolves
  the CAS-conflict path to an idempotent `Ok(false)`; the concurrent-delete test drives real spawned
  tasks. Could not break it.
- PUT-overwrite reclaim: `commit_chunk_map_superseding` (`crates/core/src/metadata.rs`) orphans the
  prior map's fragments in the *same atomic batch* and keeps the current map's fragments; the
  `gc_delete_backstop` test proves prior-gone / current-kept via placement-aware keys. Sound (modulo
  the "who runs GC" gap above).
- SigV4 correctness: the `get-vanilla`, query-sorting, and published streaming known-answer vectors
  are genuine independent oracles; `trim_all` whitespace-collapse and non-sorted `SignedHeaders`
  handling (`s3/sigv4.rs`) address the iteration-5 fail-closed erosions.
- Auth-before-body on the *reject* path (`s3/mod.rs` `handle` verifies before touching the body):
  the never-sent-body test confirms it. (The *accepted* streaming path still amplifies — see
  finding 1.)

## Note on the red→green evidence

Per `check-gates.json`, C4-verify is "red pre-fix, green post-fix," but for this net-new module the
red is a compile-error red (pre-declared acceptable). The green therefore certifies only the cases
the new tests assert; it is silent on the unbounded/malformed chunk-size path (finding 1) and on
the GC-topology gap (finding 2).

### Advisory — codex

- NEEDS-HUMAN — crates/server/src/s3/streaming.rs:201 accepts only an immediate CRLF after the zero-length aws-chunked terminator, while `crates/server/src/s3/sigv4.rs:471` classifies every `STREAMING-*` payload sentinel, including the documented `...-TRAILER` variants, as supported streaming input. A valid trailer-framed SDK upload can therefore pass seed SigV4 auth and then be rejected as malformed framing. Decide whether trailer variants are in the M4 real-SDK floor; if not, narrow `verify` to explicitly reject unsupported sentinels instead of half-accepting them.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — In-scope root cause is genuinely closed: both DELETE **and** overwrite now write an orphan grace record per superseded fragment in the *same atomic commit* (`metadata.rs:483`,`:407`), and GC reclaims only after the reader-safe window with a never-reclaim-referenced safety gate (`custodian/gc.rs:113,121`). Smell-test does not fire (removes the cause, no capability probe). **Decision owed:** across 5 iterations each Check surfaced a *new* supersede-without-orphan leak class (DELETE→overwrite); confirm no remaining path strands bytes — notably rebalance/reconstruction re-placement, which `metadata.rs:454` states is **"deliberately left non-orphaning."** Whether that is safe or the next leak variant is a durability call the human should ratify.
- [ ] T5 Judgment — Resolved this round: crypto provenance (RustCrypto `sha2`/`hmac`, already deny-allowed), crate boundary committed to `crates/server` (Cargo.toml:3). **Decisions owed:** (a) ratify the crate-boundary choice vs the architecture-named `gateway-s3` crate; (b) SigV4 accepted-variant scope + the minimal S3 error-code floor (brief Known-NEEDS-HUMAN); (c) M4-branch vs own-sequence placement; (d) the new `aws-sdk-s3` dev-dep tree passed `cargo deny` with **no deny.toml change** (introduces no denied crate) but is a sizeable new dev-time supply-chain surface worth explicit awareness.
- [x] Validation — fitness-to-purpose — **Decision owed at sign-off:** is the plaintext-loopback-at-Check posture (TLS modelled via `TlsIdentity` but unbound — `s3/mod.rs:2424-2431`) the accepted floor, with real public-TLS + live "gateway serving S3" green deferred to #367? Brief pre-declares this as off-Check, but fitness-for-first-deployment and the durability of the green gate (unquarantined lease-expiry flake, C4 caveat) are human calls the deterministic gates cannot make.
- [ ] crates/server/src/s3/streaming.rs:201 accepts only an immediate CRLF after the zero-length aws-chunked terminator, while `crates/server/src/s3/sigv4.rs:471` classifies every `STREAMING-*` payload sentinel, including the documented `...-TRAILER` variants, as supported streaming input. A valid trailer-framed SDK upload can therefore pass seed SigV4 auth and then be rejected as malformed framing. Decide whether trailer variants are in the M4 real-SDK floor; if not, narrow `verify` to explicitly reject unsupported sentinels instead of half-accepting them.

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
- Iteration delta (if iterating): Why rejected: T5(a) crate boundary is not ratified. S3 is one of many planned gateways, so the wire layer must not calcify inside crates/server. What to change next: 1. Crate boundary (decided): extract the S3 wire layer to a dedicated `gateway-s3` crate; factor a shared gateway seam the other gateways also implement; keep composition-root wiring per ADR-0010. Do not leave it in crates/server. 2. Fail-open in an in-scope feature (streaming is in scope per brief): bound the declared aws-chunked chunk size and verify the chunk signature BEFORE buffering (streaming.rs:195,210). A malformed/overflowing chunk header must return 400 InvalidRequest, never a silent truncated 200 OK. Add large-chunk + malformed-header tests. 3. Trailer-framing: either accept STREAMING-*-TRAILER or have verify reject unsupported sentinels cleanly (streaming.rs:201 vs sigv4.rs:471) — no half-accept. 4. Quarantine the gateway_lease_expiry.rs wall-clock flake so the green gate is durable. Ratified / accepted — do NOT re-litigate or spend iteration budget: - C5 causal adequacy: orphan-ledger design is sound; the custodian is a backend role, not the S3 gateway's job — the gateway correctly writes durable orphan records and the backend custodian reclaims. Rebalance/reconstruction re-place (keep) fragments, so non-orphaning there is accepted. - Validation / TLS: plaintext-loopback-at-Check + backend-run reclamation are the accepted floor; real TLS + running custodian topology deferred to #367. - T5(b) SigV4 scope: already answered by the brief — header-based only, presigned out, minimal auth-failure/not-found error floor. - T5(c) M4 sequencing: accepted, target is M4. - T5(d) aws-sdk-s3 dev-dep: accepted (cargo deny clean).
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
