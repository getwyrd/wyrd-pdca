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
