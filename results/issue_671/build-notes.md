# Build notes — issue 671 / put-object-flat-ceiling

Target: `getwyrd/wyrd @ main`, built on the integration base the driver gave this lane
(`pdca-integration/main-rc423c4501a6d` @ `596249a`, which folds #666, #686 and #767 onto
`7bf7eb9`). Line numbers below are on the patched tree in `$PDCA_WORKTREE`.

## What the change does

A single PUT is one flat chunk map, and a flat map holds at most `MAX_MAP_CHUNKS` = 158 refs.
So the largest single PUT is `158 × chunk_size`. This patch makes that cap real on every
single-PUT path:

1. **One derivation in core.** `max_single_put_bytes(c)` sits beside `max_part_bytes(c)`, and
   both read one private `one_value_bytes(c) = VALUE_CHUNK_CAPACITY × c`
   (`crates/core/src/multipart.rs:4784`, `:4793`, `:4801`). Two compile-time asserts tie
   `MAX_MAP_CHUNKS` and `MAX_PART_CHUNKS` to `VALUE_CHUNK_CAPACITY` (`:4807-4808`), so the
   two ceilings cannot drift apart (criterion 9). `MAX_MAP_CHUNKS`' doc no longer says "a
   single PutObject picks a larger chunk".
2. **Seam change (the choice the brief left to Do).** I passed the declared length *to the
   implementer* rather than pushing the ceiling *to the wire layer*:
   `ObjectGateway::put_object_streaming(key, source, declared_len, expected, content_type)`
   (`crates/gateway-core/src/lib.rs:330-341`), with two new `GatewayError` variants,
   `EntityTooLarge { declared, limit }` and `LengthMismatch { declared }` (`:266-283`).
   Why: the invariant has to bind every single-PUT path ("a check in one handler cannot
   satisfy it"). With the length on the seam, the server enforces it itself, so any future
   front door (NFS, …) gets the cap for free. Pushing the ceiling up to the S3 crate would
   have left `Gateway` trusting every caller. Cost of my choice: one extra argument on 1
   production impl + 8 test doubles (5 in `gateway-s3/src/lib.rs`, 3 in server tests), and
   1 direct caller (`s3_http_wire.rs:950`). The alternative (a `fn single_put_limit()` on the
   trait) would have touched the same 9 impls, so it was not cheaper.
3. **Server enforcement** (`crates/server/src/lib.rs`):
   - `admit_single_put` refuses `declared > max_single_put_bytes(chunk_size)` with
     `EntityTooLarge` (`:291`). Called first thing in `put_object_streaming` (`:417`),
     **before** `source` is polled, and first thing in the buffered `put_object` (`:258`),
     before `plan_write`, so no fragment is written (criterion 8a).
   - `DeclaredLengthSource` (`:921`) wraps the body: it replaces the frame that would push
     the total past the declared length with `LengthMismatch` (those bytes never reach the
     write path) and errors if the body ends short. This is the backstop that makes the
     declared length binding for any front door. Unit tests `:1078-1100`.
   - Compile-time pins (`:108-119`): `max_single_put_bytes(16 MiB)` and `max_part_bytes(16 MiB)`
     ≤ 5 GiB (so no runtime 5 GiB check is needed; raising `MAX_CHUNK_SIZE` past ~33 MiB fails
     the build), and `MAX_STAGED_CHUNKS × MIN_CHUNK_SIZE` > 10 GiB.
   - `SUPPORTED_FLAT_OBJECT_BYTES` now reads `max_single_put_bytes` too, and the
     `MIN_CHUNK_SIZE` doc no longer says an over-cap PUT "writes all its chunks and then fails".
4. **S3 wire** (`crates/gateway-s3/src/lib.rs`):
   - The PUT arm reads the declared length from the head (`:1834-1864`):
     `x-amz-decoded-content-length` for any `aws-chunked` variant, `Content-Length` for a
     single-shot body. None declared → `411 MissingContentLength`. A `Transfer-Encoding` body
     counts as "none declared" (`declared_content_length`, `:1957`). Parsing is strict digits
     only (rubric: no `+` via `from_str`).
   - `classify` maps `EntityTooLarge` → `400 EntityTooLarge`, message names the limit in bytes
     and does not mention multipart (`:3586-3591`); `LengthMismatch` → `400 IncompleteBody`.
   - `sigv4.rs`: `x-amz-decoded-content-length` moved from `TrailerDeclaration` to
     `StreamingContext.decoded_content_length` (`:103`) and is now read for **every** streaming
     variant, with a strict-digit parser (`parse_decoded_content_length`, `:590`). New public
     client helper `sign_with_payload_hash_and_headers` (`:691`) so harnesses can sign the
     decoded length like an SDK does; `sign_with_payload_hash` delegates to it with no extras
     (output unchanged — the AWS known-answer test still passes).
   - `streaming.rs`: the decoder refuses a chunk *on its header* when its declared size would
     push the decoded total past the declared length (`:369`), so no byte of it is buffered or
     forwarded; the short-body check (`check_decoded_length`, `:581`) now runs for the plain
     variant too, not only `-TRAILER`. `decode` now spawns its task **lazily** on first poll
     (`:186-225`), so a PUT the gateway refuses from its headers never reads a body byte.
5. **Docs** (docs currency rule — the seam is a port): `05-building-block-view.md` (seam
   paragraph), the M4 blueprint `--chunk-size` comment, the `wyrd s3` usage text
   (`crates/server/src/cli.rs:526-545`), and proposal 0016's carve-out, value-ceiling paragraph,
   rows X50/X102/X107, the two accepted-cost rows and the two summaries. Multipart limits are
   stated as future ones ("once it is supported"), never as a remedy available now.

## Test — `crates/server/tests/s3_put_object_ceiling.rs` (new)

Eleven tests, one 8-byte-chunk fixture each (ceiling 1,264 bytes), covering criteria 1–6 and 8a
(plus a self-check that the local signer matches production `sign`). It uses only API on the
base tree; it signs requests and `aws-chunked` chunks itself from the public crypto primitives,
because `StreamingContext` gains a field in this patch and constructing it would not compile on
base.

### Refutation (forced)

- **(a) Genuine red?** Yes. I saved the production diff, `git checkout`-ed every non-test file
  back to base, and ran the file: **9 failed, 2 passed** (10.01 s). The two that pass on base are
  the at-ceiling round trip (1,264 bytes is legal on base too) and the signer self-check, which
  is expected. The rest fail on assertions (status 200 where 400/411 expected, HEAD 200 where
  404 expected, buffered `put_object` returning `Ok`) or on the 10 s bound (legs 3 and 4, where
  base waits for a body that is never sent). Then re-applied the patch: 11/11 pass (0.52 s).
- **(b) Production path?** Yes. Every wire leg goes through the real loopback listener,
  `sigv4::verify`, `streaming::decode`, the S3 PUT arm and `Gateway::put_object_streaming` over
  `RedbMetadataStore::in_memory()` + `FsChunkStore`. Leg 8a calls `Gateway::put_object` /
  `get_object` directly. No doubles.
- **(c) Fixture includes the fault?** Yes. The backend is redb, which has no value limit, so on
  base an over-budget 159-ref map really publishes (the failure the cap exists for). Chunk counts
  are read off the real fs store's `chunks/` directory. The length-mismatch legs really stream
  more / fewer bytes than declared, and the header-time legs really send no body.

## Migrated tests (criterion 7)

Ran `cargo test --workspace` to find breakage; three tests broke, plus the known harnesses:

- `s3_http_wire.rs` `streaming_interop`: `streaming_seed` now takes the object length and
  sends it as a **signed** `x-amz-decoded-content-length`. Payloads: 4,096 → 1,000 bytes (125
  chunks, 8 frames of 128), tampered 2,000 → 1,000 bytes (frames of 256, still refused 403 for
  the tampered chunk). Real-SDK round trip 9,000 → 1,200 bytes (150 chunks). Chunk size stays 8
  so every other test in the file is unchanged.
- `s3_streaming_trailer.rs`: `streaming_seed` now always sends the decoded length (u64, not
  Option). Payloads 4,096 → 1,000 and 3,000 → 1,200, still multi-chunk/multi-frame. The
  mismatch test used `Some(9999)`, which is now over the ceiling and would be refused for its
  size; changed to `len + 1` and it now also asserts the code is `InvalidRequest`, so it is
  refused for the mismatch. I left this harness sending the decoded length **unsigned**, the
  same way it already sends `x-amz-trailer`: the gateway does not require `x-amz-*` headers to
  be signed, and the file's own doc says it is built only from base-tree signing primitives.
- `gateway-s3` unit test `an_unrenderable_committed_etag_degrades_the_put_instead_of_panicking`:
  its in-process router helper sent a PUT with no `Content-Length` (oneshot skips HTTP framing),
  so it got 411. Added the header a real client always sends.
- `validate/tests/s3_client_roundtrip.rs`: the two streaming legs now start their gateway at
  `chunk_for_single_put(len)` = `max(256 KiB, ⌈len / 158⌉)`, so the payload is always one PUT
  for any `tcp_wmem[2]`; past a 16 MiB chunk it panics with a message naming the host's
  `tcp_wmem[2]` and the limit. At this host's 4 MiB default the chunk stays 256 KiB, so nothing
  changes here. Needed `wyrd-core` as a dev-dependency (one `Cargo.lock` line); dev-deps are
  outside the blackbox guard, which passed.

## Gates

- New test red→green as above. `cargo test --workspace --no-fail-fast`: all green after the
  migrations (210 test binaries).
- `cargo xtask ci` (via `engine/xtask.sh ci`): typos, docs lint, `render_site.py --check`
  (link audit OK), gitlink/unsafe/blackbox guards, `cargo fmt --check`, clippy, the full test
  suite — all passed. It then **failed at `cargo deny … check advisories`** on two new RustSec
  advisories for `rustls 0.21.12` (via `tikv-client 0.4.0` → `wyrd-metadata-tikv`). That crate
  version is in the base `Cargo.lock` unchanged (`git show HEAD:Cargo.lock`), so base fails the
  same check today; it is an advisory-database update, not this patch. Handling it is a policy
  call (an ignore entry with a review trigger, as `deny-all-features.toml:111` does for
  RUSTSEC-2026-0104) and out of this slice's scope.
- A second full `cargo xtask ci` on the final tree gave the same result: every stage through
  `cargo test` (200 test binaries ok, 0 failed), `cargo-machete` and the default `cargo deny
  check` passed; only the all-features advisories check failed, as above. Because `ci` stops
  there, I ran the two later stages directly through the same runner:
  `xtask conformance` → "5 valid + 6 invalid vectors pass"; `xtask dst` (the `--cfg madsim`
  build) → all green (33 + 8 + 9 + 1 passed, 1 ignored). No DST code calls any API this patch
  changes.
- `typos` and `render_site.py --check` (the brief's two external dependencies) are both present
  and pass.

## Ruled out

- **Per-PUT chunk-size selection** — rejected by the human at Plan (brief).
- **Ceiling on the wire layer only** (a `single_put_limit()` method the S3 crate checks) —
  leaves `Gateway` unbounded for any other front door; same number of impls touched.
- **Using hyper's `size_hint().exact()` as the declared length** — hyper reports `Some(0)` for
  a PUT with neither `Content-Length` nor `Transfer-Encoding`, which would treat "no length" as
  "length 0" instead of 411. Reading the headers is explicit.
- **Checking the decoded length only at end of stream** (today's trailer behaviour) — would
  let a stream write past its declaration before failing; the check now runs per chunk header.

## Open points for the human

- The `cargo deny advisories` failure above is pre-existing and unrelated; someone needs to
  decide on the `rustls 0.21` advisories (ignore with review trigger, or bump `tikv-client`).
- `streaming::decode`'s spawned task is still not aborted when its stream is dropped
  (pre-existing; lazy spawn means refused PUTs never start it, but an aborted write mid-stream
  still leaves the task to end at its next send). Not widened by this patch.
- Release ordering (brief): until #668–#670 ship, an object above `158 × chunk_size` has no S3
  path in; the help text and blueprint say a larger `--chunk-size` is the remedy.
