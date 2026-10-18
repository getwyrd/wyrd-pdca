# Design proposal — issue 671 / put-object-flat-ceiling

> The Plan artifact (design-proposal form; enhancement). Do reads ONLY this file. The
> `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **RE-SCOPED at Plan (2026-10-14, with the human).** As filed, #671 makes a single `PutObject`
> **pick** a chunk size per request: `max(DEFAULT_CHUNK_SIZE, ⌈Content-Length / MAX_MAP_CHUNKS⌉)`,
> plus a start-time precondition `chunk_size_max ≥ ⌈5 GiB / MAX_MAP_CHUNKS⌉` for streams with
> no length (proposal 0016 decision 7, rows X102/X107). Two things landed after that was
> written, and they rule it out:
>
> - **The chunk size is now a deployment property.** PR #848 (merged 2026-10-07) made
>   `wyrd s3 --chunk-size N` the gateway's one chunk size, checked at startup to lie in
>   1–16 MiB (`crates/server/src/cli.rs:2323-2327`, `:2597-2625`). Its own comment calls it "a
>   deployment property (the erasure-coding unit and the per-object fan-out)". Per-PUT picking
>   would quietly turn that flag into a minimum.
> - **The numbers 0016 assumed do not hold.** A flat map holds `MAX_MAP_CHUNKS` = **158**
>   chunks (`crates/core/src/multipart.rs:4759`), not 165–381, and a chunk is at most **16 MiB**
>   (`crates/server/src/lib.rs:55-72`, set by the D servers' 4 MiB gRPC message limit), not
>   13–32 MiB. So `⌈5 GiB / 158⌉` ≈ 32.4 MiB is a chunk size no deployment can run, and X107's
>   start-time precondition could never be met.
>
> The human chose: **chunk size stays the startup setting; a single PUT is capped at 158 × that
> size; anything larger is multipart's job** (#508's slices). **A PUT must declare its length, as
> S3 requires**, so the cap can be checked from the headers. This slice makes that cap honest.
> Citations verified against `getwyrd/wyrd` `origin/main` @ `7bf7eb9` on 2026-10-14.

- **Slug:** put-object-flat-ceiling
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** A single PUT that cannot fit one flat chunk map at the gateway's configured chunk
  size is refused with `400 EntityTooLarge`, from the headers, before any of its bytes are
  written. Today nothing enforces the cap. A single PUT is one flat map, and a flat map is sized
  to hold 158 chunk refs (`MAX_MAP_CHUNKS`, "never an over-budget value",
  `crates/core/src/multipart.rs:4755-4759`). That sets the ceiling at 158 × chunk size: 158 MiB at
  the default 1 MiB, about 2.47 GiB at the 16 MiB maximum. Yet no code path compares a PUT
  against it. The constant is referenced only by the server's chunk-size floor
  (`crates/server/src/lib.rs:34`, `:87`, `:100`), and the commit path never checks it. What
  happens to a larger PUT depends on the backend:
  - on FoundationDB, every byte is written, and then the commit fails once the record passes the
    100,000-byte value limit (`crates/server/src/lib.rs:88-96` states this);
  - on redb, which has no value limit, the PUT **publishes** a flat map past the 158-entry budget
    every cap in the code is sized for (`crates/core/src/metadata.rs:546-575`).

  Two gaps keep the cap from being checked up front:
  - **A PUT may omit its length.** A streamed `aws-chunked` upload's object size is
    `x-amz-decoded-content-length`, not its framed `Content-Length`. The gateway reads that header
    only for the `-TRAILER` variants (`crates/gateway-s3/src/sigv4.rs:509`, `:564-590`), and treats
    it as optional there. The plain `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` form (`:549`) never
    reads it. S3 requires it, and AWS's SDKs and CLI always send it.
  - **A declared length is not held to.** Where it is read, it is compared with the streamed bytes
    only after the whole body has arrived (`crates/gateway-s3/src/streaming.rs:532-538`).
- **Success criterion:** A NEW wire-level test over the in-process gateway, built with
  `.with_chunk_size(8)` so the ceiling is 158 × 8 = **1,264 bytes** (the peer suites use a small
  chunk size the same way):
  (1) a PUT declaring exactly 1,264 bytes (`Content-Length`) succeeds and round-trips byte for
  byte;
  (2) a PUT declaring 1,265 bytes answers `400` with S3 error code `EntityTooLarge`, and nothing is
  stored: HEAD is `404`, and the chunk store directory holds no chunk for it;
  (3) a signed PUT that **declares** `Content-Length: 2147483648` and never sends a body answers
  `400 EntityTooLarge` within a few seconds. That proves the refusal comes from the headers, not
  from reading the body;
  (4) a streamed `aws-chunked` PUT whose `x-amz-decoded-content-length` is 1,265 answers
  `400 EntityTooLarge` the same way, at the header;
  (5) a PUT that declares **no** object length is refused from the headers with
  `411 MissingContentLength` (S3's error for a PUT without a declared length), and nothing is
  stored. That covers a streamed `aws-chunked` upload of **any** variant without
  `x-amz-decoded-content-length` (a small 100-byte body is enough), and a body sent with
  `Transfer-Encoding: chunked` and no `Content-Length`;
  (6) a streamed `aws-chunked` upload that declares `x-amz-decoded-content-length: 1000` (under
  the ceiling) and then streams 1,265 bytes is refused, and nothing is published (HEAD `404`). The
  gateway stops once the declared length is passed, so at most the declared length's worth of
  chunks is written, not the whole stream. One that streams **fewer** bytes than declared is
  refused too;
  (7) **every existing test still tests what it tested.** Two kinds of existing test break under
  this change, and both are migrated so that each keeps its own purpose:
  - **Streamed PUTs with no decoded length** are updated to send it, the way the AWS SDKs do:
    the interop harness in `crates/server/tests/s3_http_wire.rs` (`streaming_seed`, `:1001`) and
    the trailer harness in `crates/server/tests/s3_streaming_trailer.rs`, whose `streaming_seed`
    sends it only optionally (`:356-399`).
  - **Tests whose object is larger than 158 × their own chunk size** would now be refused
    `EntityTooLarge` before reaching the check they exist for. Every peer suite uses an 8-byte
    chunk (ceiling 1,264 bytes) or a 4-byte one (632 bytes). Known cases on `7bf7eb9`:
    `s3_http_wire.rs` (chunk 8, `:65`) streams 4,096 bytes and expects success (`:1070`),
    streams 2,000 tampered bytes and expects `403` (`:1101`, `:1116`), and has the real SDK
    round-trip 9,000 bytes (`:1190-1200`); `s3_streaming_trailer.rs` (chunk 8, `:101`) expects
    4,096- and 3,000-byte uploads to succeed (`:452`, `:504`); and
    `crates/validate/tests/s3_client_roundtrip.rs` (chunk 256 KiB, `:109`, ceiling 39.5 MiB)
    sizes its streaming payload from the host's `tcp_wmem[2]` (`:139-159`): about 37 MiB at the
    4 MiB default, so a host with a larger send buffer would cross the ceiling. Do runs the full
    gate to find any others.
  - **How to migrate:** raise the fixture's chunk size, or shrink its payload, so the object is
    under its ceiling, while keeping what the test needs: an object that spans **several storage
    chunks**, a body that spans **several wire frames**, and each refusal test refused for its
    **own** reason (forged signature, tampered chunk, tampered checksum, unknown algorithm …),
    not for the length or the ceiling. The validate test's payload must stay under its gateway's
    ceiling for any `tcp_wmem[2]` it reads, and should fail with a clear message if it cannot.
    The new boundary tests (1)–(6) keep their own dedicated 8-byte fixture.
  - The real-SDK round trips stay green.

  (8) the `--chunk-size` help text and the M4 blueprint's `--chunk-size` comment state the size
  limits the configured value `c` implies, as numbers for the default and the maximum, **without
  presenting multipart as available**. On `main` and after this slice, every multipart form still
  answers `501 NotImplemented` (`crates/gateway-s3/src/lib.rs:403-413`, `:1777-1783`); the verbs
  land with #668–#670. So:
  - the largest single PUT, 158 × `c`, is stated as **the** limit an object has today;
  - the largest multipart part, `max_part_bytes(c)` = 158 × `c`, and the largest multipart
    object, `MAX_STAGED_CHUNKS` × `c` = 80,896 × `c` in at most 10,000 parts, are stated as the
    limits multipart **will** have once it is supported, not as a remedy available now.

  That is 158 MiB / 158 MiB / about 79 GiB at 1 MiB, and about 2.47 GiB / 2.47 GiB / 1.23 TiB at
  16 MiB. Until multipart ships, the operator's only way to accept a larger object is a larger
  `--chunk-size`, and the help text says so. The `EntityTooLarge` answer to a single PUT names its
  limit in bytes. It does not tell the client to use multipart while this gateway answers
  multipart with `501`;
  (8a) **the buffered path is bounded too.** A direct call to `Gateway::put_object`
  (`crates/server/src/lib.rs:235-256`, the buffered path that does not go through the S3 arm) on
  the same 8-byte-chunk gateway: 1,264 bytes succeeds and `get_object` returns it; 1,265 bytes
  returns `Err`, the chunk count under the fs store's `chunks/` is unchanged, and `get_object`
  returns `None`. The test asserts only `Err` here, not the error's type, so it still compiles on
  the reverted base (where `put_object` exists and stores the 1,265 bytes, which is the red). The
  error's type is pinned by the wire legs, which map it to `EntityTooLarge`;
  (9) the single-PUT ceiling and the multipart part ceiling come from **one** derivation in
  `wyrd-core`, so PutObject and UploadPart can never disagree about what one value holds.
- **Falsifiability:** RED on `origin/main`, in-process, on `RedbMetadataStore::in_memory()`:
  - redb accepts a 159-chunk flat map, so (2) stores and publishes the object (HEAD `200`);
  - (3) and (4) do not refuse at the header, so the gateway waits for the body until the test's
    timeout;
  - (5) is accepted and stored today;
  - in (6), the plain `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` form ignores the declared length, so
    all 1,265 bytes are stored;
  - in (8a), `Gateway::put_object` stores and publishes 1,265 bytes (159 chunks) and returns
    `Ok`.

  Every leg fails on an assertion or a timeout. The test uses only pre-existing API (the wire,
  plus `Gateway::put_object` / `get_object`, which `crates/server/tests/e2e.rs:38-41` already
  calls), so C4-verify compiles it on the reverted base.
- **Invariant to restore:** No single PUT publishes a flat chunk map of more than
  `MAX_MAP_CHUNKS` entries, on **any** backend. A PUT declares its object length, as S3
  requires. A PUT whose declared length would cross the bound is refused before any of its
  bytes are written. A PUT is never allowed to stream past its declared length. **Sources:**
  `MAX_MAP_CHUNKS`' own contract (`crates/core/src/multipart.rs:4755-4759`); the value-budget
  rule the cap is derived from (`crates/core/src/metadata.rs:546-575`, a cap sized to the
  tightest backend); `AGENTS.md:160-163`, *Protocol input*: "oversize input is indeterminate or an
  error — never silently accepted. Enforce declared `Content-Length`". SELF-TEST: it binds every
  single-PUT path (the S3 streamed and signed forms, and `Gateway::put_object`), so a check in one
  handler cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 620, 666, 768
- **Ordering note:** No build-on dependency: #655's constant is on `main`, and the verbs are not
  needed. Conflicts:
  - `Conflicts with 620`: both add a header-time refusal to the same S3 PUT arm
    (`crates/gateway-s3/src/lib.rs:1765-1830`).
  - `Conflicts with 666`: telling the gateway the declared size before streaming is expected to
    change the PUT seam on `ObjectGateway` (`crates/gateway-core/src/lib.rs:291-309`). That touches
    every implementer and test double #666 extends, and the same S3 bounds.
  - `Conflicts with 768`: both may edit the gateway's PUT path (`crates/server/src/lib.rs:234-410`).

  So this runs in a later wave than #620 and #666, and in a different wave from #768.
  Outside this batch: the PLANNED #508 brief still lists PutObject chunk-size *selection* in its
  scope (its leg D). That is superseded by this re-scope, and #508's brief needs narrowing at its
  next Plan pass. **Intake-cap override (wyrd-pdca-P1):** the human allowed all seven ids of this
  batch (656, 666, 671, 767, 768, 620, 686) in over the alpha cap in the 2026-10-14 Plan session.
  Count at that moment: alpha 4/6 planned (508, 625, 633, 654), room 2, need 7.
- **Scope:** refusing a single PUT past the flat-map ceiling implied by the configured chunk
  size, from its headers; requiring every PUT to declare its object length, as S3 does; holding
  every streamed upload to its declared length while it streams; the same bound on the buffered
  `Gateway::put_object`; migrating the in-tree tests that send streamed PUTs without the decoded
  length, and those whose objects exceed 158 × their own chunk size, so each keeps its purpose
  (criterion 7); the single-PUT ceiling derived in core beside the multipart part ceiling. Also the operator- and design-facing text that states the size limits the chunk size implies (single PUT, part, multipart object): the `--chunk-size` help, the M4
  blueprint comment, and proposal 0016's PutObject carve-out / out of scope: choosing a chunk size
  per PUT (rejected above); raising `MAX_CHUNK_SIZE` or the gRPC message limit; S3's 5 GiB
  single-PUT limit as a separate runtime check (unreachable while the ceiling is at most about
  2.47 GiB; see Design); segmented maps for single PUTs (multipart-only, 0016 decision 7);
  every multipart verb (#666–#670); the D servers.
- **Difficulty:** medium
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/server/tests/s3_put_object_ceiling.rs` — a **NEW** file (C4-verify
  treats only an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **Peers to mirror:**
  - Refusing before the body is read, with a test that declares a large `Content-Length`, sends
    nothing, and bounds the wait: `crates/server/tests/s3_http_wire.rs:265-296`
    (`unsigned_put_is_refused_before_its_body_is_read`).
  - A header-time refusal on the PUT arm: CopyObject's, at `crates/gateway-s3/src/lib.rs:1800-1806`.
  - The streamed PUT with no decoded length, which (7) updates: the interop harness at
    `crates/server/tests/s3_http_wire.rs:1001-1094` (`streaming_seed`,
    `stock_sdk_chunked_put_round_trips_byte_identical`). As an `x-amz-*` header, the decoded
    length has to be among the signed headers.
  - Where the decoded length is read today: only for `-TRAILER` variants, in
    `parse_trailer_declaration` (`crates/gateway-s3/src/sigv4.rs:509`, `:564-590`). The streaming
    variants are listed at `:547-555`. It is enforced only at the end of a trailer stream
    (`crates/gateway-s3/src/streaming.rs:532-538`), where the decoder already counts `decoded_len`
    (`:394`).
  - Counting chunks on disk: PR #848's repro counted directories under the fs chunk store's
    `chunks/`.
  - The S3 PUT arm streams through `put_object_streaming` at `crates/gateway-s3/src/lib.rs:1829-1868`.
  - The gateway's chunk size is `Gateway::chunk_size` (`crates/server/src/lib.rs:117`).
  - The core streaming writer that cuts chunks is `write::stream_write_data`, called at
    `crates/server/src/lib.rs:372-385`.
- **Prior-art check (triage cycles):** searched by path `crates/gateway-s3/src/lib.rs` (PUT arm),
  `crates/server/src/lib.rs` (`MAX_CHUNK_SIZE`, `MIN_CHUNK_SIZE`, `put_object_streaming`),
  `crates/server/src/cli.rs` (`parse_s3_chunk_size`) and the 0016 carve-out. Merged: PR #848 (the
  `--chunk-size` flag and its 1–16 MiB bounds) and `f8634de` (the 1 MiB floor "so single PUTs
  stay publishable"). Neither refuses a PUT past the ceiling. Proposal 0017 (`5682572`) lists
  `EntityTooLarge` as unimplemented and cites #671. #508's rejected monolithic attempts carried
  per-PUT selection as their leg D. Open PRs on getwyrd/wyrd: none. No PR on #671.
- **Disposition hint:** new-feature

Plan-review response: all three findings checked against `7bf7eb9` and revised in place.
(1) Test migration: criterion (7) now covers tests whose objects exceed 158 × their own chunk
size (the reviewer's four, plus the `validate` round-trip whose payload scales with
`tcp_wmem[2]`), with the rule for keeping each test's purpose. (2) Buffered path: new criterion
(8a) calls `Gateway::put_object` directly at 1,264 and 1,265 bytes. (3) Multipart: criterion (8)
and the `EntityTooLarge` message no longer present multipart as available, and Impact states
the release ordering and the `--chunk-size` remedy in the meantime.

## Motivation

A client that sends a single PUT larger than the gateway can publish should hear so at once,
not after uploading every byte. On FoundationDB today it hears only after uploading every byte.
On redb it is not told at all: the object is published with a map over the size every backend
is budgeted for. The ceiling is a direct consequence of two settled facts: the flat-map budget
(158 refs) and the operator's chosen chunk size. The honest behaviour is to state it and enforce
it. Objects above it are what multipart is for. At the default 1 MiB chunk, a multipart upload
holds up to `MAX_STAGED_CHUNKS` = 512 × 158 = 80,896 chunks, about 79 GiB
(`crates/core/src/metadata.rs:544`, `crates/core/src/multipart.rs:4818`), which covers 0016's
">10 GiB" launch requirement. The AWS CLI and SDKs already switch to multipart above 8 MiB by
default.

## Design

**The ceilings, one derivation.** The configured chunk size `c` sets every object-size limit the
deployment has, single PUT and multipart alike, and there is no per-request chunk size:
- **single PUT:** `MAX_MAP_CHUNKS × c`, because a flat map is one value;
- **one part:** `MAX_PART_CHUNKS × c`, already in core as `max_part_bytes`
  (`crates/core/src/multipart.rs:4774-4782`), because a `part:` record is one value;
- **one multipart object:** `MAX_STAGED_CHUNKS × c` (`:4809-4818`), the segmented ceiling, in at
  most `MAX_PARTS_PER_SESSION` parts.

All three caps are `VALUE_CHUNK_CAPACITY`-based (`:4731-4772`). Put the single-PUT ceiling in
core **beside** `max_part_bytes`, built from the same rule, rather than as a second formula in
the server or the S3 crate. The gateway, its help text and the blueprint then all read one
source. #669 (UploadPart) will use the part ceiling. At the 1 MiB floor the multipart object
ceiling is 80,896 MiB, about 79 GiB, above 0016's ">10 GiB" launch requirement for every
`--chunk-size` the gateway accepts. Pin that relation at compile time
(`MAX_STAGED_CHUNKS × MIN_CHUNK_SIZE` over 10 GiB), so lowering the floor or the caps cannot
silently drop below it.

**Declared size.** The size is declared by `Content-Length` for a single-shot body, or by
`x-amz-decoded-content-length` for `aws-chunked`; an `aws-chunked` body's `Content-Length`
counts its framing, not the object. A declared size over the ceiling is refused with
`400 EntityTooLarge` before the body is read. The S3 crate does not know the gateway's chunk size
(it sees only the `ObjectGateway` seam), so the declared size has to reach the implementer before
it streams, or the ceiling has to reach the wire layer. Either is a seam change in
`gateway-core`, which is why this conflicts with #666. Do picks which and records it in
`build-notes.md`. Map the refusal to `400 EntityTooLarge` where the PUT arm maps gateway errors
today (`gateway_error_response`).

**No declared size.** Refused from the headers with `411 MissingContentLength`, before any body
byte is read. S3 requires a declared length on PutObject: `Content-Length` for a single-shot
body, and `x-amz-decoded-content-length` for every `aws-chunked` variant. AWS's SDKs and CLI
always send it. This replaces the issue's "today's lengthless streaming PUT keeps working"
acceptance (the human's call at Plan). In 0016's terms it is X107's header-time refusal, made
unconditional.

**The declared size is held to.** A declared length is only worth checking against the ceiling if
the stream cannot exceed it. A single-shot body is already framed by `Content-Length`. For
`aws-chunked`, hold the decoder to `x-amz-decoded-content-length` **while** it streams: refuse as
soon as the decoded bytes pass it, not after the whole body. Keep refusing a stream that ends
short of it, which is today's end-of-stream check (`crates/gateway-s3/src/streaming.rs:532-538`)
extended to every variant. A refused stream is never published, and any leased fragments it
already wrote go to the same lease-expiry cleanup that reclaims any failed PUT today.

**Every single-PUT path.** The same bound applies to `Gateway::put_object`, the buffered path,
which knows its length up front (`crates/server/src/lib.rs:230-256`; caller `cli.rs:1884`). It
builds its own plan (`plan_write`, `:236`) rather than going through `put_object_streaming`, so a
check on the S3 arm alone does not cover it; criterion (8a) tests it directly. Refuse before
`plan_write`, so no fragment is written.

**S3's 5 GiB limit** (on a single PUT, and on one part). Neither can be reached while
`158 × MAX_CHUNK_SIZE` (about 2.47 GiB) is below it, so a separate runtime check would be dead
code. Pin the relation at compile time instead, the way `crates/core/src/multipart.rs:4752-4754`
pins its own budget. Then raising `MAX_CHUNK_SIZE` past about 33 MiB fails the build until a
5 GiB check arrives with it.

**Text to correct in the same PR:**
- `MAX_MAP_CHUNKS`' doc ("a single `PutObject` picks a larger chunk instead",
  `crates/core/src/multipart.rs:4756-4758`);
- the `--chunk-size` help (`crates/server/src/cli.rs:516-523`);
- the M4 blueprint comment (`docs/design/architecture/m4-first-deployment-blueprint.md:1147-1152`);
- proposal 0016's single-PUT carve-out (`0016:2296-2316`) and its rows X50 (`:2582`), X102
  (`:2633`), X107 (`:2638`), the accepted-costs rows (`:2859-2860`) and the summaries at
  `:3032` and `:3084`. Each says the single PUT uses the deployment chunk size, must declare
  its length (X102's size-independent chunk for length-less streams is withdrawn), and is refused
  past 158 × that size, citing PR #848 for why selection was dropped.

0016 is a **draft** proposal, so it is edited in place; no ADR is touched.

## Alternatives considered

- **Per-PUT chunk-size selection (as filed; 0016 decision 7)** — contradicts `--chunk-size` as a
  deployment property (PR #848), and still cannot reach 5 GiB under the 16 MiB cap. Rejected by
  the human at Plan.
- **Accept length-less PUTs and stop them at the 159th chunk** — keeps today's small length-less
  streams working, but a size the gateway cannot see up front cannot be refused up front, and S3
  itself requires the length. Rejected by the human at Plan in favour of requiring it.
- **Raise the gRPC message limit first, so single PUTs reach 5 GiB** — a separate change with its
  own costs (bigger fragment files on disk, more gateway memory per chunk). Not needed for the
  >10 GiB target, which multipart meets. Not this slice.

## Impact & compatibility

Client-visible:
- a single PUT over the ceiling now gets `400 EntityTooLarge` up front, instead of a late failure
  or a silent over-budget publish;
- a PUT that declares no length gets `411 MissingContentLength`. Only non-standard clients are
  affected; AWS's SDKs and CLI always declare it;
- a streamed upload that sends more or fewer bytes than it declared is refused.

PUTs within the ceiling that declare their length are unchanged, and so is how objects are
chunked. No record or config change. The ceiling is the same number operators already set through
`--chunk-size`; the help text now says it.

**Release ordering, stated.** This slice has no build dependency on multipart, and it may ship
before #668–#670. While it does, an object above the single-PUT ceiling has **no** S3 path into
the store: multipart still answers `501`. On FoundationDB that changes little (such a PUT already
fails, only later). On redb it removes something that "worked": an over-budget flat map that
published. The remedy in that window is a larger `--chunk-size` (up to about 2.47 GiB at 16 MiB),
and the help text and blueprint say so. If the human wants the cap held back until multipart
ships, that is a release decision, not a change to this slice.

## Open questions

1. **0016 edits in this PR.** This brief corrects the draft proposal's single-PUT text in the
   same PR, so the design and the code agree. If the human prefers a separate docs change, drop
   the 0016 bullet from scope.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
