# Brief (pointer) — issue 364 / s3-http-wire-surface

> A Plan artifact that is a **pointer**: the planning decision lives in governed design
> corpus — the accepted M4 proposal (0015) and the **m4-first-deployment-blueprint** (the
> real spec for the endpoint this issue builds). This file references them and carries the
> fields the driver parses; Do reads the **Planning artifact** as the authoritative plan and
> does not restate it. NOTE: unlike a scheduled slice, #364 is **UNSCHEDULED load-bearing
> work** — no milestone/proposal/issue books it — surfaced by the 2026-07-01 roadmap review
> as a gap gating the first deployment (#367). See the MODEL brief at
> `results/issue_256/brief.md` for the field discipline this pointer follows.

- **Slug:** s3-http-wire-surface
- **Planning artifact:** the deployment corpus, read in place under `../wyrd` (never copied).
  Authoritative for *what to build*:
  - `docs/design/architecture/m4-first-deployment-blueprint.md` — **the real spec**: the
    **Gateway** is the "**Stateless S3 front door**" that "embeds the client library"
    (blueprint:59); it is dialed `--s3-listen 0.0.0.0:8080` with a **PUBLIC** S3 TLS cert
    that is *separate* from the internal step-ca mTLS fabric (blueprint:620–623, ADR-0036
    req 5); "**the S3 endpoint is the only thing that needs public exposure**"
    (blueprint:196–199); the day-one **Definition-of-Done** requires "**gateway serving
    S3**" and a "**Round-trip: S3 PUT an object → GET it back byte-identical**"
    (blueprint:698–699); the honest scope statement to a first user is "a single-zone,
    production-durable, **S3-compatible** object store" (blueprint:382).
  - Architecture **§5** building-block view — "S3 gateway | Rust, **`gateway-s3` crate** |
    **Primary integration surface**" (`05-building-block-view.md:132`); **§7.5**
    ports/protocols — `App → S3 gateway | **HTTP / S3** | operator-set (e.g. 443, 8080 in
    dev) | **TLS; S3 SigV4**` (`07-deployment-view.md:72`); **§8.5** security/trust and the
    **§14 threat model** — external-principal auth is "**OIDC / S3 SigV4 / mTLS**" at the
    gateway (`14-threat-model.md:86,111`).
  - Proposal **0015** (accepted, supersedes 0007) — the **scope-boundary** citation, not the
    build spec: its *Deployment prerequisite* note states plainly that "**the gateway exists
    only as `put`/`get` client mode**" and that runnable gateway process roles are a **named
    prerequisite** outside M4's *metadata* scope (`0015-…-revised.md:443–463`, esp. 451). This
    issue builds the wire half of that prerequisite — the **runnable gateway server role**; per
    the **2026-07-04 maintainer decision**, #364 (this issue) + #366 (custodian process) satisfy
    0015's process-role half, so no separate process-roles issue is needed. Also relevant: the OOM cliff on
    "everything on the network into the gateway's heap" (0015:789) → streaming bodies.
- **Defect / goal:** the gateway has **no client-facing network endpoint**. Today it is an
  **in-process library** (`Gateway<M,C,Co>` with `put_object`/`get_object`,
  `crates/server/src/lib.rs:44,114,146`) plus a `put`/`get` **client mode** in the CLI
  (`crates/server/src/cli.rs:57–58`); the crate docstring itself records "PUT/GET are exposed
  as in-process methods at M0; the **HTTP/S3 wire surface is a later milestone**"
  (`crates/server/src/lib.rs:1–9`). Nothing can talk to a deployment **over the network**.
  Give the gateway a real client-facing HTTP endpoint: **bucket-scoped object PUT / GET /
  DELETE over S3-compatible HTTP, with request signing (SigV4) and TLS**, mapping onto the
  existing in-process client write/read paths — so the blueprint's day-one S3 round-trip
  (blueprint:698–699) can actually run over the wire.
- **Success criterion:**
  - **BINDING (demonstrable at Check):** an S3-compatible HTTP listener in the gateway role
    accepts a **bucket-scoped, SigV4-signed** request and completes a **PUT → GET → DELETE
    round-trip**, with **GET returning the PUT bytes byte-identical** (blueprint:699); and a
    request with a **missing/invalid signature is refused** (no anonymous access). Wire
    encoding (HTTP framework, exact S3 status/XML shapes) is **ILLUSTRATIVE**; the binding
    conditions are "the gateway serves bucket-scoped object PUT/GET/DELETE over S3-compatible
    HTTP", "unsigned/bad-sig is rejected", and "the object survives a byte-identical
    round-trip over the network."
  - **DEFERRED (off-Check — see STOP discipline):** the gateway **serving real S3 to a
    public client over TLS on a deployed host** (blueprint DoD:696–699). Per 0015's
    Deployment-prerequisite note the *multi-node, discovery-driven* stand-up is gated on work
    outside this issue (etcd-backed `Coordination` + the other process roles); at Check the
    listener is exercised over **loopback with a self-signed / test cert**, and the live
    public-TLS green is observed at the **first-deployment gate (#367)**.
- **Repo + branch target:** getwyrd/wyrd @ `feat/m4-production-metadata-backend`
  (resolves to the M4 integration branch, `origin/feat/m4-production-metadata-backend` →
  225d3bd at brief time). This first-deployment-gate work lands on the M4 integration base
  exactly as slice 5 / #256 does; the slice's own branch PRs **into** this base, not `main`.
  (Whether it should instead sit in its own sequence between M4 and M7 is a §NEEDS-HUMAN
  call — the item note pins the branch to M4.)
- **Depends on:** the **in-process gateway library** it wraps — `Gateway::put_object` /
  `get_object` (`crates/server/src/lib.rs:114,146`) and the client write/read paths
  (`wyrd_core::{read,write}`). It in turn **gates #367** (the first-deployment gate): #367's
  day-one runbook cannot run without a network S3 endpoint.
- **Conflicts with:** any concurrent restructuring of `crates/server` (the crate-boundary
  decision below overlaps it); and the M4 metadata slices only insofar as they touch the same
  gateway composition root (`crates/server/src/lib.rs`, the ADR-0010 concrete-wiring point).
  No trait/on-disk-format conflict.
- **Scope:** an S3-compatible **HTTP listener** in the gateway role; **bucket-scoped**
  object **PUT / GET / DELETE**; **SigV4 authentication** (signature verification mandatory);
  **TLS** on the public S3 port per §7.5; **streaming** request/response bodies (no
  full-object buffering, per 0015:789). The verbs map onto the existing in-process client
  paths — DELETE is net-new (`delete_object` does **not** exist today, confirmed absent in
  `crates/server/src/lib.rs`), so it also adds the delete mapping.
- **Out of scope:** the **full S3 API surface** — multipart upload, `ListObjectsV2` +
  pagination, conditional requests, presigned URLs, ACLs beyond signing, an S3 error-code
  conformance sweep (all deferred to pre-M8; "Full S3 semantics (buckets/ACLs/multipart) are
  deferred" — `crates/server/src/lib.rs:1–9`); the **portal / manageability** (M8);
  **auth/OIDC beyond wire signing** (a later milestone — §14:111 lists OIDC as separate);
  the **Deployment prerequisite** proper (etcd-backed `Coordination` + custodian process
  role — 0015:443–463); any change to `traits`, the on-disk format, or the `MetadataStore` /
  `Coordination` contracts.
- **Ordering note:** builds the **wire seam ahead of its live consumer**. The endpoint is
  authored and exercised over loopback now; the LIVE "gateway serving S3 publicly" green
  (blueprint DoD:696–699) is observable only at the first-deployment gate (#367), which also
  needs the separate coordination prerequisite (0015:443–463). This is BUILT and
  load-bearingly exercised at Check (signed round-trip succeeds; unsigned is refused), not
  dead scaffolding.
- **Do model:** opus-xhigh
- **Difficulty:** **hard** — net-new **network protocol surface** (HTTP listener, S3 request
  routing, streaming bodies) plus **security-sensitive** SigV4 verification and TLS. Larger
  blast radius than a `deploy/`-only slice: it introduces the gateway's first public wire and
  its auth boundary.
- **Test file:** `crates/server/tests/s3_http_wire.rs` (path ILLUSTRATIVE — **confirm at
  build**; if the crate-boundary call creates a `gateway-s3` crate, the test moves with it,
  e.g. `crates/gateway-s3/tests/round_trip_signed.rs`). The flippable at-Check regression: an
  S3 client drives **PUT → GET → DELETE over the real HTTP listener on loopback**, asserting
  (a) GET returns the PUT bytes byte-identical and (b) an **unsigned / wrong-signature request
  is rejected** — RED before the wire surface exists (no listener to dial / anonymous access),
  GREEN after. Do must supply a *demonstrated* red where feasible (net-new coverage).
- **Citations expected:** Do must cite `path:line` on the target branch **and** the Planning
  artifact for every change — e.g. the existing in-process seam it wraps
  (`crates/server/src/lib.rs:114,146` and the `:1–9` "later milestone" marker it retires),
  the CLI client mode (`crates/server/src/cli.rs:57–58`), and the spec lines it satisfies
  (blueprint:59, 620–623, 698–699; `07-deployment-view.md:72`; `14-threat-model.md:86`).
- **Disposition hint:** likely-fix — a net-new, spec-anchored feature landing behind the
  first-deployment gate; carries pre-declared NEEDS-HUMAN items (crate boundary, SigV4 scope)
  rather than surprise ones.

## Invariants to hold
- **Reuse the client path, don't reimplement it.** The HTTP verbs map onto the existing
  `Gateway::put_object` / `get_object` (`crates/server/src/lib.rs:114,146`) and the
  `wyrd_core::{read,write}` paths; the round-trip is byte-identical (blueprint:699).
- **ADR-0010 concrete-wiring stays in one place.** The HTTP layer is generic over
  `Gateway<M,C,Co>` (`lib.rs:44`); concretes are picked only at the composition root (`main`
  / the server profile), never re-wired in the wire layer.
- **Auth is fail-closed.** SigV4 signature verification is **mandatory** — an unsigned or
  bad-signature request is refused; there is **no anonymous S3 access** (§7.5:72, §14:86).
- **Two distinct TLS identities.** The **public** S3 cert is separate from the **internal**
  step-ca mTLS fabric (blueprint:620–623, ADR-0036 req 5) — do **not** conflate the public
  S3 TLS listener with internal service-to-service mTLS.
- **Stream, don't buffer.** Request/response bodies stream; no full-object buffering into the
  gateway heap (0015:789 OOM cliff).
- **Bucket-scoped keys** map onto the flat namespace as it exists today (`ROOT`,
  `crates/server/src/lib.rs`) — **confirm at build** how bucket + key compose onto the M0 flat
  root; do not invent a directory tree.

## Known NEEDS-HUMAN
- **Crate boundary:** land the wire surface **inside `crates/server`** (M0's "combined
  `server`, split later" posture, ADR-0016 as the concrete-wiring designator — see 0015:147)
  **or** stand up the **`gateway-s3` crate** that architecture §5:132 already names? Human
  decision; it determines the test path.
- **SigV4 scope:** which signing variants the floor must accept (header-based vs presigned
  query — presigned is out of scope) and which S3 auth version — **confirm**.
- **Sequencing:** does this sit on the **M4 integration branch** as a first-deployment-gate
  slice (item note's pin) or in its **own sequence between M4 and M7**? (Mirrors the MODEL
  brief #256's open question 3.) Human call.
- **Error-code floor:** the S3 error-code **conformance sweep** is out of scope, but *which
  minimal subset* the floor must return correctly (e.g. auth failure, not-found) is a human
  call — real S3 compatibility is a known rabbit hole meriting its own pre-M8 gate.

## STOP discipline
- Builder MAY push to a feature/draft branch and open a **draft** PR into
  `feat/m4-production-metadata-backend` for CI. Builder MUST NOT mark the PR ready or merge —
  that is the human's sign-off.
- **Security surface — do not weaken auth to make a test pass.** The demonstrated red
  (unsigned/bad-sig request refused) is load-bearing; a green obtained by disabling signature
  verification or TLS is not acceptable.
- **Off-Check green is not a deliverable gap.** The gateway serving real S3 to a public
  client over TLS on a deployed host is observed at the **first-deployment gate (#367)** (and
  needs the separate coordination prerequisite, 0015:443–463) — its absence at Check is
  pre-declared here, not a surprise NEEDS-HUMAN.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: The binding Check floor passes (signed PUT->GET->DELETE byte-identical; unsigned/bad-sig -> 403; auth boundary survived forge attempts; crypto pinned to FIPS/RFC/AWS vectors) — keep all of that. But items 1-3 alone warrant iteration, and the rebuild should address all of the following: 1. "Stream, don't buffer" invariant is INVERTED and streaming was IN scope (not deferred). `to_bytes(body, MAX_BODY_BYTES)` (s3/mod.rs:167) materializes the whole request body (up to 256 MiB) before any gateway work, and GET buffers the whole object (`Gateway::get_object` -> Vec<u8>, lib.rs:149) into `Body::from`. The 256 MiB cap is per-request, so k concurrent PUTs -> k*256 MiB resident (the 0015:789 OOM cliff). Deliver true streaming PUT/GET (widen the `put_object`/`get_object` seam off `&[u8]`/`Vec<u8>` as needed) rather than a buffering-only floor. 2. Round-trip proves self-consistency, not S3 compatibility. The test signs with the gateway's own `sigv4::sign`, which shares `canonical_request` with `verify`, so canonicalization bugs are invisible. The sorted/percent-encoded canonical query string and URI-encoded path rules AWS requires are un-anchored — `canonical_request` interpolates raw query/uri (sigv4.rs:163,260). A real aws-sdk/boto3 request with query params diverges -> 403. Implement proper SigV4 canonicalization (sorted + URI-encoded query, URI-encoded path) and add an independent oracle: a known-answer vector with a non-empty/needs-encoding query, and ideally a real-SDK interop test. 3. Concurrent DELETE is not idempotent, contradicting its own contract. `unlink` CAS-requires the dirent unchanged (metadata.rs:319), so two concurrent DELETEs of the same key give first=204, second=409 OperationAborted where S3 returns 204. Make delete idempotent (treat the not-found/conflict-into-absent race as success) and add a concurrent/retry test. 4. Vendored hand-rolled SHA-256/HMAC on the auth boundary (s3/crypto.rs) was chosen to keep `cargo deny` green. Resolve the crypto-provenance decision: either run the RustCrypto dependency audit (ADR-0003 three-test + deny.toml allowlist) and use a vetted crate, or record explicit sign-off to keep the vendored implementation on a security surface. 5. TLS is modeled but unwired — `TlsIdentity` is carried but never bound; Check runs plaintext loopback, while the brief's "self-signed/test cert at Check" wording expected a cert. Wire the loopback TLS listener (public/deployed TLS remains deferred to #367). 6. Pre-auth memory amplification + replay residual: the body is read before `sigv4::verify` (s3/mod.rs:650 precedes :662), so an unsigned request can force up to 256 MiB allocation before rejection — verify auth before materializing the body (fits the streaming rework). Also add `x-amz-date` freshness/skew (replay window); note UNSIGNED-PAYLOAD leaves the body outside the signature on the plaintext wire. Net-new coverage red is a compile-error red (acceptable for a net-new module), but the assertions above must become behavioral (streaming, real-SDK canonicalization, concurrent DELETE). </content>
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the demonstrated floor is not yet fit for the first-deployment gate. The rebuild must CORRECT the following, not re-defer them a second time. T5 governance (all three must be corrected): - (a) crypto provenance: replace the hand-rolled SHA-256/HMAC on the auth boundary with a vetted RustCrypto crate — run the ADR-0003 three-test audit / update deny.toml. Silent re-deferral is not acceptable again. - (b) crate-boundary: pick and commit the S3 wire layer's home (gateway-s3 vs crates/server) rather than leaving it an open ratification. - (c) SigV4 scope: correct to the required floor (header-only / error-code subset), addressing the real-SDK gaps named below. Adversary findings to fix in the rebuild: - Streaming is implemented but never behaviorally demonstrated: the only wire round-trip is 55 bytes / 8-byte chunk and passes identically for a buffering impl. Add a PUT of an object > DEFAULT_CHUNK_SIZE (lib.rs:41) with a bounded-memory / per-chunk-observation assertion. - DELETE leaks committed chunk fragments AND unlink's doc comment (crates/core/src/metadata.rs:305) falsely claims "GC reclaims" — the sweep scans only pending: keys. Correct the false comment. - Canonicalization is proven only at unit level; no real-SDK request is exercised. Fix the two named real-client breaks: percent-encoded key identity (boto3 key "my file.txt" stored/keyed as "my%20file.txt") and STREAMING-AWS4-HMAC-SHA256- PAYLOAD misclassification (400 + chunk-framing stored as object data). Add a real-SDK interop path. - XML error bodies (codex) interpolate `message` without escaping; attacker- controlled SignedHeaders can inject markup. Escape the error message. Accepted — do NOT spend iteration budget here: - TLS: plaintext-loopback-at-Check is accepted; TLS wiring comes later (human decision). Do not re-wire TLS this iteration. - Replay-within-15-min and UNSIGNED-PAYLOAD-on-plaintext are pre-declared residuals tied to the accepted TLS deferral.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: gating C4-ci is recorded red (cargo test --workspace exit 101), and beyond the gate there are two real DELETE correctness defects. Direction for the rebuild: - DELETE crash-leak: delete_object reclaims only via reclaim_fragments and writes NO orphan-ledger grace record (server crate doesn't depend on custodian), so a crash between the unlink commit and reclaim_fragments strands the object's fragments permanently — custodian GC never reclaims them (no orphan record, no pending lease). The "GC backstop" comment is false; make the backstop real (write an orphan/grace record so GC can reclaim) or stop claiming it. - Placement-aware delete: delete_object deletes by fragment index (ChunkStore::delete_fragment), but the read/write path is placement-aware; after fragment movement/rebalance the reclaim targets the wrong D-server and leaks. Delete from the placed D-server per ChunkRef (placement-aware delete counterpart). - C4 gate: re-run cargo test --workspace --exclude wyrd-dst on the current target and localize. The adversary re-ran clean (exit 0, 82 binaries green), so the red is likely stale or a pre-existing timing flake (gateway_lease_expiry.rs / gateway_cluster.rs), NOT this diff — but the recorded gate must be re-run green before any accept; do not lean on the per-fix run-verify green (it only exercises the new test). - Carry-forward judgments to fold in: real-SDK interop is still asserted, not proven (round-trip signs with the gateway's own sign(); a default modern SDK PUT hits the 501 STREAMING path), and the crate-boundary choice (crates/server vs the named gateway-s3 crate) remains a pre-declared human call — surface both again next Check. §6 items: none ticked — cannot accept while the gating gate is red; the open items are the reject basis.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Gating C4-ci is RED on the record (`cargo test --workspace --exclude wyrd-dst` exit 101), so no green gate exists to accept against. Diagnosis: the failure is a non-reproducible, wall-clock-sensitive flake in a PRE-EXISTING, untouched test (`crates/server/tests/gateway_lease_expiry.rs`, which reads the real wall clock and asserts now+ttl vs now-ttl) — NOT attributable to this diff. Both reviewers re-ran the workspace green (adversary: 83 binaries green; new tests 5x/15x/3x green); the bundle captured only "exit status: 101" with no failing-test name. Likely aggravated by cross-lane host-clock skew (see §10). The merits are strong: DELETE crash-leak backstop, delete idempotency under 64-round races, RustCrypto provenance, auth-before-body, and behavioral streaming all survived adversarial attack. Re-run C4-ci to an authoritative GREEN gate and localize/quarantine the wall-clock flake so the record matches. Also address before re-accept (human calls owed at next Check): - Real-SDK interop still unproven: the round-trip signs with the gateway's own sigv4::sign; only AWS KAT vectors are the independent oracle (pins signature math, not wire framing). A stock modern aws-sdk streaming PUT (STREAMING-AWS4-HMAC-SHA256-PAYLOAD) is rejected 501 — so "S3-compatible" holds only for single non-chunked signed bodies. Add a real boto3/aws-sdk path or ratify the scope explicitly. - Concurrent GET-during-DELETE truncation: happy-path DELETE reclaims fragments eagerly, ignoring the orphan grace window the code advertises; a slow multi-chunk GET can be truncated mid-object by a concurrent DELETE. Decide whether that is acceptable first-deployment S3 semantics or DELETE must honour the grace window / GET must take a version-hold.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected (iter-5): gates are green and prior blockers are genuinely fixed (RustCrypto sha2/hmac, crate boundary committed, SigV4 AWS-KAT-pinned, DELETE crash-leak root cause closed) — but the adversary found a new permanent leak on the happy path plus two carried-forward gaps that must be closed before accept. (1) BLOCKING — PUT overwrite leaks the prior object's fragments permanently. `commit_written` (`crates/server/src/lib.rs:182-202`) routes an overwrite of an existing key to `write::commit_overwrite` (`crates/core/src/write.rs:265-272`), which only CAS-swaps the new chunk map onto the inode and NEVER calls `reclaim_fragments` and NEVER writes an orphan grace record (orphan records exist only in `metadata::unlink`, `crates/core/src/metadata.rs:394-411`). After `PUT /b/k` (A) then `PUT /b/k` (B), A's fragment bytes are stranded on every D-server forever, on the happy path, no crash. This is the SAME leak class the last three iterations fixed for DELETE, now reopened on the more common verb — and worse (no crash required). There is NO test. Fix: route overwrite through the same orphan-grace-record + GC-reclaim path DELETE uses, and add an overwrite-reclaim regression test (mirror the DELETE reclaim tests at `crates/server/tests/s3_http_wire.rs:372,816`). (2) BLOCKING — GET-during-DELETE truncates a streaming read (codex + C5, carried UNADDRESSED from iter-4). `delete_object`'s eager reclaim (`lib.rs:245` → `reclaim_fragments` → `delete_fragment_at` `lib.rs:285`) deletes fragments immediately, and `lib.rs:286` clears the orphan grace records immediately, bypassing the reader-safe grace window the GC design rests on (`gc.rs:136`, 0005:291-294). A concurrent `get_object_streaming` (`lib.rs:312-334`) resolves the chunk map up front then reads lazily; if DELETE lands mid-stream, `read_chunk_verified` raises `MissingFragment` (`crates/core/src/read.rs:145,172`) and the reader sends a truncated body — no `Content-Length` set and `200 OK` already emitted (`s3/mod.rs:255-259`), so the client cannot distinguish truncation from success. Single-chunk objects truncate to zero bytes. This directly undercuts the binding byte-identical round-trip under concurrent access. Make DELETE honour the grace window (leave fragments under the orphan ledger until GC's grace expires) or have GET take a version-hold; add a GET-during-DELETE regression. (3) BLOCKING — real-SDK interop is asserted, not demonstrated. The over-the-wire round-trip/streaming tests sign with the gateway's OWN `sigv4::sign` / `sign_with_payload_hash` and frame with the gateway's own helper (`crates/server/tests/s3_http_wire.rs:598,633`, comment :21) — no real boto3/aws-sdk process ever hits the listener. The unit KATs (`s3/sigv4.rs:629,659`, `s3/streaming.rs:278`) pin the signing math but not the wire framing/header set a live SDK emits. The "stock SDK round-trips instead of 501-ing" claim (mod.rs:19-23) is verified only against the gateway's model of an SDK. Recurring carry-forward from iterations 2-4. REQUIRED at-Check path (maintainer decision at sign-off): add the real Rust `aws-sdk-s3` (+ `aws-config`) as a dev-dependency of `crates/server`, point its endpoint at the loopback listener with path-style addressing and static creds, and drive `put_object` → `get_object` → `delete_object` asserting byte-identity — a genuine independent oracle whose signer/framer is NOT `crates/server`'s own `sigv4`/streaming helpers (that self-reference is exactly what was refuted). This runs under plain `cargo test`, no container, deterministic. Before committing, check the `aws-sdk-s3` dependency tree against `deny.toml` / INTEGRATION.md §4 (license/allowlist) — flag as a NEEDS-HUMAN if it introduces a denied crate. A live boto3/aws-cli Tier-2 leg stays a pre-declared DEFERRED backstop, not the at-Check bar. While here, fix the minor fail-closed SDK-compat erosions the adversary flagged: `verify` does not collapse sequential internal whitespace in signed header values (`s3/sigv4.rs:403`) and re-sorts `SignedHeaders` (`sigv4.rs:384-385,405`), so a real client signing doubled internal spaces or sending non-sorted SignedHeaders gets a spurious 403. Not re-litigated (standing human calls owed at the next Check, not blocking the rebuild): T5 SigV4 scope / minimal S3 error-code floor; M4 sequencing; Validation/TLS plaintext-loopback-at-Check posture with public TLS deferred to #367 (rustls-provider deny.toml/license decision when TLS is wired). Also confirm at the next Check that the historical `gateway_lease_expiry.rs` wall-clock flake is quarantined so the gate green is durable (reviewer could not re-run cargo here).
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Why rejected: T5(a) crate boundary is not ratified. S3 is one of many planned gateways, so the wire layer must not calcify inside crates/server. What to change next: 1. Crate boundary (decided): extract the S3 wire layer to a dedicated `gateway-s3` crate; factor a shared gateway seam the other gateways also implement; keep composition-root wiring per ADR-0010. Do not leave it in crates/server. 2. Fail-open in an in-scope feature (streaming is in scope per brief): bound the declared aws-chunked chunk size and verify the chunk signature BEFORE buffering (streaming.rs:195,210). A malformed/overflowing chunk header must return 400 InvalidRequest, never a silent truncated 200 OK. Add large-chunk + malformed-header tests. 3. Trailer-framing: either accept STREAMING-*-TRAILER or have verify reject unsupported sentinels cleanly (streaming.rs:201 vs sigv4.rs:471) — no half-accept. 4. Quarantine the gateway_lease_expiry.rs wall-clock flake so the green gate is durable. Ratified / accepted — do NOT re-litigate or spend iteration budget: - C5 causal adequacy: orphan-ledger design is sound; the custodian is a backend role, not the S3 gateway's job — the gateway correctly writes durable orphan records and the backend custodian reclaims. Rebalance/reconstruction re-place (keep) fragments, so non-orphaning there is accepted. - Validation / TLS: plaintext-loopback-at-Check + backend-run reclamation are the accepted floor; real TLS + running custodian topology deferred to #367. - T5(b) SigV4 scope: already answered by the brief — header-based only, presigned out, minimal auth-failure/not-found error floor. - T5(c) M4 sequencing: accepted, target is M4. - T5(d) aws-sdk-s3 dev-dep: accepted (cargo deny clean).
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: issue_364 — the feature floor is sound (signed loopback PUT->GET->DELETE, differentiated auth codes, streaming fail-closed), but two codex durability findings MUST be resolved before this ships — they sit exactly on the M4 production-metadata-backend durability seam: 1. Durable id allocator / startup recovery. `Gateway::new` resets `next_inode`/`next_chunk` to 1 on every process start (crates/server/src/lib.rs:85) over persistent redb/fs state. After a restart, new-key PUTs spuriously conflict against existing inode ids, and overwrites can mint chunk ids that already back committed objects before the CAS publishes the new map -> corruption/loss. Add a durable allocator or recover the high-water marks from metadata/chunk state at startup. 2. Per-chunk lease deadlines. The streaming PUT computes one 30s expiry and stamps it on every chunk (crates/core/src/write.rs:436; server lib.rs:209). A slow authenticated upload can run past that deadline before commit, letting a concurrent custodian sweep reclaim early chunks as expired pending garbage and publish an object with missing fragments. Stamp each chunk relative to its own write time / renew leases / hold reclaim until commit. Add durability tests that actually exercise these — a restart-then-GET/PUT round-trip and a slow-PUT-under-custodian-sweep. The current loopback suite exercises neither, which is why both defects passed green. Ratified this iteration — do NOT re-litigate or re-defer: - C4 gate red was the `gateway_lease_expiry` wall-clock flake, not a regression: clean-host `cargo test --workspace --exclude wyrd-dst` = EXIT 0, lease-expiry test 5/5 green (see §6.1/§6.4). - The minimal S3 error-code floor is sufficient for durability testing (see §6.2). Keep it; do not chase the full S3 conformance sweep. Also worth folding in (adversary, same fault-under-durability theme): streaming GET emits 200 OK + partial body on a mid-stream fragment fault (crates/server/src/lib.rs:238-256; gateway-s3/src/lib.rs:262-266) with no S3 error code — decide buffer-to-first-error / trailer-error / Content-Length while reworking the above.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: Close the high_water_marks orphan-ledger blind spot before this lands: metadata::high_water_marks scans only `inode:` and `pending:` prefixes, so after PUT → DELETE → restart the deleted chunk ids appear in no scanned prefix and Gateway::recover() re-mints them — stale `orphan:` records then either leak permanently (GC's `protects` gate skips them forever) or reclaim a not-yet-committed fragment of the new object (data loss). Fix direction per adversary review: also scan `orphan:` (project the chunk id, take the max) so re-mint never lands on an id whose orphan record / on-disk fragments are still live. Add a behavioral red covering DELETE (or crash mid-overwrite) followed by restart — the existing recover tests only cover PUT → restart → PUT-new-key. Everything else is ratified and must not be re-litigated: shared-core placement of the durability seam on feat/m4-production-metadata-backend is accepted (§6.1 ticked); crate boundary, SigV4 scope, TLS deferral stand from iter-6; plaintext-loopback-at-Check posture is otherwise acceptable pending #367. Do not rework the wire surface — this is a targeted fix to recovery plus its test.
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
