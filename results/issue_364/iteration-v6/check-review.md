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
