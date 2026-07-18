# Build notes — issue 503 / object-metadata-model (iteration 4)

## What this iteration changes (and what it does NOT)

Iteration-3 sign-off explicitly said: *"the implementation and design stand, do not
redesign. Fix exactly this: … the GET arm degrades a malformed stored `content_type` … but
still passes the stored `etag` UNGUARDED into the response builder …, so a stored etag
containing a non-header byte (e.g. CR/LF …) panics every GET of that object at the
`.expect("streaming response is always valid")`. Apply the symmetric fallback: when the
quoted etag is not a valid header value, omit the ETag header (degrade, never panic), and
add a router-level test mirroring the malformed-content_type one."*

So iteration 4 is the **whole iteration-3 patch, unchanged, plus one focused hardening
fix and its test.** I re-applied `iteration-v3/patch.diff` verbatim onto the base and made
only these additions on top:

1. **`etag_header` helper** (`crates/gateway-s3/src/lib.rs:735` on the patched tree, doc
   comment above it) — `fn etag_header(etag: &str) -> Option<HeaderValue>`, returning `Some`
   when `HeaderValue::from_str(&quote_etag(etag))` succeeds and `None` otherwise. This is the
   exact symmetric analogue of the existing `content_type_header`
   (`crates/gateway-s3/src/lib.rs:747`) that iteration 3 already added for the content-type
   field.

2. **GET arm now degrades instead of panicking** (`crates/gateway-s3/src/lib.rs:653`, inside
   the `Ok(Some(ObjectRead { .. }))` block whose builder is `.expect(...)`-built at `:661`).
   Was:
   ```rust
   if let Some(etag) = &etag {
       builder = builder.header("etag", quote_etag(etag));
   }
   ```
   Now:
   ```rust
   if let Some(value) = etag.as_deref().and_then(etag_header) {
       builder = builder.header("etag", value);
   }
   ```
   An un-renderable stored etag yields `None`, the `if let` is skipped, and the ETag header
   is simply omitted — the object still serves. A well-formed etag renders exactly as
   before, so the happy path (and every existing test) is unchanged.

3. **A router-level test** `a_malformed_stored_etag_degrades_the_get_instead_of_panicking`
   (`crates/gateway-s3/src/lib.rs`, in `#[cfg(test)] mod tests`), mirroring the sibling
   `a_malformed_stored_content_type_degrades_the_get_instead_of_panicking`. It seeds a
   stored etag carrying a literal `\r\n` byte, drives a **signed GET through the real
   router** (`.oneshot(...)` → production GET dispatch arm → response builder), and asserts
   `200` + body served in full + **no** `ETag` header. To share the router-drive plumbing
   with the sibling test I extracted a `signed_get_through_router(Arc<StoredMetaGateway>)`
   helper and gave `StoredMetaGateway` an `etag` field (with a `StoredMetaGateway::new()`
   well-formed baseline that each test perturbs on exactly the field it exercises). The
   content-type test is updated to use the shared helper — behaviour identical, still
   asserts the ETag is surfaced for its well-formed etag.

Nothing else in the iteration-3 patch was touched: the `InodeRecord` model, the
publication-vs-repair commit split, the four repair-preservation tests, the two overwrite-
freshness unit tests (`mutation_regressions.rs`), the wire round-trip tests
(`s3_object_metadata.rs`), the seam widening, `http_date`, and ADR-0047 all stand as
accepted at prior sign-offs.

## Why guard-and-omit, not something heavier

The alternative "remove the cause" would be to **enforce the constraint at the seam** —
reject or sanitize a non-header-safe etag when `put_object_streaming` commits it. But this
path never mints a bad etag: the server computes the etag as `hex(&hashing.finalize())`
(`crates/server/src/lib.rs`), strictly `[0-9a-f]`, always a valid header value. The
malformed value is only reachable via **stored corruption / out-of-band edits**, which
ADR-0045 says the decode boundary treats *liberally* (parse-don't-validate, liberal read).
A seam-time guard cannot cover a value that was written correctly and later corrupted on
disk; the wire layer is the last line that must not panic on read. So the cause ("a stored
byte the write path did not put there") lives outside this code's control, and the correct
minimal fix is exactly the symmetric wire-level degrade the sibling `content_type` field
already uses — restoring the invariant *"a malformed stored metadata value degrades the
GET response, never panics it"* uniformly across all three metadata fields. Cost of the
degrade: +16 lines (`etag_header` + the 3-line call-site change). Cost of a seam-enforcement
approach that still would not close the corruption path: a new validation type on the public
`ObjectGateway` seam (#504/#506 callers absorb it) plus *still* needing this wire guard for
the corruption case — strictly larger and it does not remove the cause.

## Refute-your-own-test (forced, recorded)

**(a) Genuine red?** YES. With the guard reverted to the unconditional
`builder.header("etag", quote_etag(etag))`, the new test fails by **panic** at
`crates/gateway-s3/src/lib.rs:661:22` — `panicked at … "streaming response is always
valid: http::Error(InvalidHeaderValue)"` — the exact `.expect(...)` the carry-forward
named. Re-applying the guard makes it pass. (The dead-code lint on the temporarily-unused
`etag_header` was silenced with `RUSTFLAGS="-A dead_code"` for the red run only; that lint
is orthogonal to the behaviour under test.)

**(b) Production path?** YES. The test builds the real `S3Gateway::new(...).router()` and
sends a genuinely SigV4-signed GET through `.oneshot(...)`. The code exercised — signature
verification, object dispatch, the GET response-builder header assembly, and the
`.body(...).expect(...)` — is all production `handle`-path code in
`crates/gateway-s3/src/lib.rs`. `StoredMetaGateway` stands in only for the *storage
backend* (the `ObjectGateway` impl), exactly as the accepted sibling content-type test
does; the panic and the fix both live in the production wire layer, not in the double.

**(c) Fixture includes the fault?** YES. The fixture seeds `etag: Some("0badc0de\r\ninjected:
header")` — a real CR/LF byte, the precise malformed value that makes `HeaderValue::from_str`
fail and (pre-fix) poison the response builder. The fault is injected, not curated out; the
oracle asserts the object still serves *and* that the ETag header is absent, so a fix that
merely swallowed the body would not pass.

## Test runner / environment

Ran through `cargo test -p wyrd-gateway-s3 --lib` and `cargo test -p wyrd-server --test
s3_object_metadata` in `$PDCA_WORKTREE` (the project gate is `cargo xtask ci` over the same
tree; the Bash tool provides the timeout). All load-light — the gateway-s3 lib tests pull
in only axum/tower/tokio, no GUI/display. `cargo fmt -p wyrd-gateway-s3` and
`cargo clippy -p wyrd-gateway-s3 --all-targets` are clean (commit-ready for the target's
hooks).

## Pre-declared sign-off item (unchanged from prior iterations)

ADR-0047 (`docs/design/adr/0047-object-metadata-model.md`) ships in this patch and is a
project-defined human-only item (INTEGRATION §4). The reviewer will route it to §6
NEEDS-HUMAN; that is expected, not a defect. Its decisions (SHA-256 opaque ETag, flat
optional record, status `Accepted` in-slice) were approved at prior sign-offs (T5) and are
not reopened here.
