## Summary
**User impact:** S3 clients cannot upload large objects to Wyrd. Multipart upload is how
tools like the AWS CLI and most SDKs send big files, and Wyrd answers every multipart request
with `501 Not Implemented`. This PR does not change that yet. Clients see exactly the same
behavior before and after.

What it does: it adds the internal interface that multipart support will plug into, with every
operation answering "not implemented" for now. This is the first of the slices that split #508.
The next ones fill in the operations one at a time (#668 create/abort/list, #669 upload part,
#670 complete), and #667 hardens the routing that will send requests here.

## What to look at
- **The new interface** in the gateway core crate: six multipart operations and the answers
  each one can give. This is the contract the later PRs build on, so the names and answer
  shapes are the main thing to review.
- **The server's implementation**: six short "not implemented" bodies, each with a comment
  naming the PR that replaces it.
- **Everything else** is mechanical: the S3 crate now requires the new interface, and the
  existing test doubles get empty implementations.

To try it: `cargo test -p wyrd-server --test multipart_gateway_seam` runs all six operations
against a real server gateway, then sends all six multipart request forms to a live S3 listener
and checks that each still gets the same 501 and that none reaches the new interface.

## Root cause
There was no multipart interface between the S3 wire layer and the server. The S3 crate turns
every multipart form away with a 501 through its subresource denylist
(`crates/gateway-s3/src/lib.rs:410-413` on `main`), so multipart support had nowhere to go
without widening `ObjectGateway`, which ADR-0046 decision 6 and ADR-0010 keep object-only.

## Fix
- `crates/gateway-core/src/lib.rs:433-803`: a new `MultipartGateway` trait, built like
  `ContainerGateway` (`Send + Sync + 'static`, `impl Future … + Send` methods, a default body for
  each method so a gateway with no multipart support needs only an empty `impl`). Its answers
  mirror `wyrd_core::multipart`'s outcomes arm for arm, using plain types so `gateway-core`
  still does not depend on `wyrd-core`. Every outcome is a value (for example
  `MultipartCreateAnswer::Refused(MultipartRefusal::NotImplemented)`); `Err` is kept for store
  failures. `NamedParts` (`:656-676`) holds Complete's part list and cannot be built empty, so
  S3's `MalformedXML` case is answered before the seam is called. No enum is
  `#[non_exhaustive]`, so a new arm breaks every wire mapping at compile time.
- `crates/server/src/lib.rs:757`: `Gateway` implements `MultipartGateway`. Each verb is written
  out and answers `NotImplemented`.
- `crates/gateway-s3/src/lib.rs`: `MultipartGateway` added to the gateway bound at `:229`
  (`S3Gateway::new`), `:797`, `:1527`, `:1638`, `:2002` and the test helper at `:5148`. Empty
  impls for the test doubles (`:3711`, `:4007`, `:4697`, `:5203`, `:5325`). The denylist and
  both guards are not touched.
- `crates/server/tests/request_capacity_planes.rs`: empty impls for `FaultyGateway` and
  `MidStreamFaultGateway`, and the helper bound.
- `docs/design/architecture/05-building-block-view.md:137`: the building-block view names the
  new trait.

Name mapping from core, for the follow-up PRs: core's `NoSuchBucket` is `NoSuchContainer`;
`digest` is `etag`; the answer enums are prefixed (`MultipartCreateAnswer`, …) so they do not
clash with core's names. The seam adds two refusals core does not have:
`PartNumberOutOfRange` and `NotImplemented`.

## Verification
- **Claim:** the server answers every multipart operation with a typed "not implemented".
  **Checked:** `crates/server/src/lib.rs:757-840`, one stub per verb.
  **Test:** `the_composition_root_answers_every_multipart_verb_not_implemented`
  (`crates/server/tests/multipart_gateway_seam.rs:109`) calls all six on a real `Gateway` over
  `RedbMetadataStore::in_memory` and compares each answer exactly. Changing one stub to answer
  `Refused(NoSuchUpload)` makes it fail (`left: Refused(NoSuchUpload) right:
  Refused(NotImplemented)`). A second test runs the trait's six default bodies the same way.
- **Claim:** the S3 crate requires the new trait, and the real `Gateway` meets that bound.
  **Checked:** the bound lines in `crates/gateway-s3/src/lib.rs` listed under Fix (source review;
  Rust has no stable negative trait test, and the workspace has no compile-fail harness).
  **Test:** `the_real_gateway_meets_the_s3_crates_gateway_bound` (`:243`) builds `S3Gateway` over
  the real `Gateway` and serves all six forms on loopback.
- **Claim:** the wire is unchanged and no multipart request enters the new interface.
  **Checked:** the denylist at `crates/gateway-s3/src/lib.rs:410-413` is not in the diff.
  **Test:** `every_multipart_form_still_answers_501_without_entering_the_seam` (`:388`) sends
  `POST /b/k?uploads`, `PUT /b/k?partNumber=1&uploadId=u`, `POST /b/k?uploadId=u`,
  `DELETE /b/k?uploadId=u`, `GET /b/k?uploadId=u` and `GET /b?uploads` to a gateway that counts
  calls. Each gets the 501 naming its subresource, and the count is 0 afterwards. Making the
  `uploads` form call the seam and still answer 501 turns this test red (`left: 1 right: 0`),
  which the status alone would miss. Two more tests check that the response parser rejects 18
  kinds of broken framing, so a loose parser cannot make these pass.
- **Claim:** an empty Complete list cannot reach the seam, and `NamedParts` keeps every part in
  the order sent. **Test:** `an_empty_complete_list_cannot_reach_the_seam` (`:129`), and two
  unit tests in `crates/gateway-core/src/lib.rs:805-822`. `cargo mutants --in-diff`: 10 caught,
  6 unviable, 0 missed.
- **Pre-fix:** the new test does not compile on `main`, because the trait does not exist yet
  (`E0432: unresolved imports`). The failures above were produced by mutating the patched tree.
- **Full gate:** `cargo xtask ci` currently fails on `main` itself, before and after this PR, at
  `cargo deny --all-features --config deny-all-features.toml check advisories`:
  RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`, which only the optional
  `tikv` feature pulls in through `tikv-client 0.4.0`. This PR does not touch any manifest,
  `Cargo.lock` or deny config. The waiver will be its own PR. Every other step passes on this
  branch: typos, docs lint and render, the guards, fmt, clippy, build, `cargo test` (1,685 tests,
  0 failed), cargo-machete and `cargo deny check`. `ci` stops at the first failure, so
  conformance and statics were run on their own and pass. DST does not build any crate this PR
  touches.

Fixes #666
