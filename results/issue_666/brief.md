# Design proposal — issue 666 / multipart-gateway-seam

> The Plan artifact (design-proposal form; enhancement). Do reads ONLY this file. The
> `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> Slice 1 of 6 of #508 (seam v, the S3 wire surface): 666 seam → 667 routing hardening → 668
> Create/Abort/ListParts/ListUploads → 669 UploadPart → 670 Complete; 671 PutObject sizing is
> independent. This slice adds the **seam only**. Every verb answers a typed "not implemented",
> and the wire does not change. Citations verified against `getwyrd/wyrd` `origin/main` @
> `7bf7eb9` on 2026-10-14.

- **Slug:** multipart-gateway-seam
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** A `MultipartGateway` companion trait in `crates/gateway-core`, mirroring
  `ContainerGateway` (`crates/gateway-core/src/lib.rs:389-431`). It carries the six multipart
  verbs in neutral container/object vocabulary. `wyrd-server`'s `Gateway` implements it at the
  composition root, beside its existing `ContainerGateway` impl
  (`crates/server/src/lib.rs:683-747`), and the S3 crate requires it of its gateway type, so the
  four later slices only fill in bodies. ADR-0046 decision 6 and ADR-0010 keep `ObjectGateway`
  object-only and bucket-free, so a new concern arrives as a narrow companion trait. Today there
  is no multipart seam at all; the S3 crate refuses every multipart form `501` through its
  subresource denylist (`crates/gateway-s3/src/lib.rs:410-413`).
- **Success criterion:** A NEW test shows legs (1)–(3), and the full gate shows leg (4).
  **(1) The composition root answers every verb, typed.** Calling each of the six
  `MultipartGateway` methods on a real `wyrd_server::Gateway` (over `RedbMetadataStore::in_memory`,
  as the peer suites build it) returns the seam's typed "not implemented" answer. That is a
  value the caller matches on, not a panic, not `todo!()`, and not an untyped boxed error.
  **(2) The real `Gateway` satisfies the S3 crate's bound.** A generic helper bounded on
  `ObjectGateway + ContainerGateway + MultipartGateway` accepts the real `Gateway`, and
  `S3Gateway::new` over it compiles. This leg proves only that the real `Gateway` meets the
  triple bound. It does **not** prove the S3 crate *requires* `MultipartGateway`: today's
  `S3Gateway::new` (`crates/gateway-s3/src/lib.rs:227-232`) already accepts a gateway that
  happens to implement it. That property is assigned to **source review at Check**: the diff
  must show `MultipartGateway` added to `S3Gateway::new`'s `where` clause and to every `G:` bound
  listed under `Citations expected`. Rust has no stable negative trait test, and the workspace
  has no compile-fail harness (no `trybuild`, no `compile_fail` doctest), so this slice does not
  add one.
  **(3) The wire is unchanged, and the seam is not entered.** Over the in-process listener, with
  `S3Gateway` serving a **sentinel gateway** whose six `MultipartGateway` methods each record
  that they were called (a counter held in a struct field, not a `static`), every multipart form
  still answers `501 NotImplemented` naming its subresource: `POST /b/k?uploads`,
  `PUT /b/k?partNumber=1&uploadId=u`, `POST /b/k?uploadId=u`, `DELETE /b/k?uploadId=u`,
  `GET /b/k?uploadId=u`, `GET /b?uploads`. After all six, the sentinel's counter is **zero**.
  The counter is what catches a request that enters the seam and then maps the "not
  implemented" answer to the same `501`, which the status alone cannot. The sentinel's
  `ObjectGateway` methods may simply answer an error, as `FaultyGateway` does
  (`crates/server/tests/request_capacity_planes.rs:189-218`); none is reached.
  **(4) The full gate is green.** `cargo xtask ci` (the C4-ci row) passes. That is the tracker's
  acceptance (`notes.json`: "`cargo xtask ci` green") and the repo's Rust gate (`AGENTS.md:41-54`).
  It is the only command that builds the S3 crate's own `#[cfg(test)]` doubles
  (`crates/gateway-s3/src/lib.rs:3516-3517` onward, and the helper bound at `:5141`), which
  `cargo test -p wyrd-server` never compiles. Any step the gate skips for a missing local tool
  (it warn-skips `typos`, `cargo-machete`) is listed as skipped in `build-notes.md`, never
  reported as green.
- **Falsifiability:** (1) and (2) cannot compile on `main`, because the trait does not exist;
  that is the red, but C4-verify cannot run it (see `Verification posture`). (3) is green before
  and after on purpose: it pins "routing untouched", and its sentinel counter goes red if any
  multipart form reaches a `MultipartGateway` method, whatever status the request then answers.
  Legs 1–3 run in-process on the plain `cargo test -p wyrd-server` target; leg 4 is C4-ci. No
  special environment.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 671
- **Ordering note:** Wave 0. There is no build-on dependency: the trait needs nothing from #656,
  and the server answers "not implemented" without calling core. `Conflicts with 671`: #671 is
  expected to change the PUT seam on `ObjectGateway` (to carry the declared length), which touches
  every implementer and test double this slice also extends, and the same `G:` bounds in
  `crates/gateway-s3/src/lib.rs` (`:229`, `:797`, `:1527`, `:1638`, `:2002`). So they go in
  different waves. #620 and #767 edit other regions of `crates/gateway-s3/src/lib.rs`, and #686
  edits a different function in `crates/server/src/lib.rs`; none of those hunks overlap. Outside
  this batch: the PLANNED #508 bundle's brief still describes this trait as part of its own
  scope, so when this lands, #508's brief needs narrowing at its next Plan pass. **Intake-cap
  override (wyrd-pdca-P1):** the human allowed all seven ids of this batch (656, 666, 671, 767,
  768, 620, 686) in over the alpha cap in the 2026-10-14 Plan session. Count at that moment:
  alpha 4/6 planned (508, 625, 633, 654), room 2, need 7.
- **Scope:** the multipart companion trait and its neutral vocabulary in `gateway-core`, the
  composition-root implementation in `wyrd-server` answering "not implemented" for every verb,
  the S3 crate's bound on it, and every existing implementer and test double updated to satisfy
  that bound / out of scope: any routing change (the denylist at
  `crates/gateway-s3/src/lib.rs:410-413` and both guards stay exactly as they are; that is #667);
  any S3 XML, status or error-code mapping for these answers (#668–#670); any call into
  `wyrd_core::multipart` (#656–#659 provide those functions; #668 wires them); a `wyrd-core`
  dependency for `gateway-core` or `gateway-s3`; the PutObject sizing of #671.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/server/tests/multipart_gateway_seam.rs` — a **NEW** file (C4-verify
  treats only an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Verification posture:** (a) net-new. The trait is a new public symbol, so the test cannot
  compile on C4-verify's reverted base, and C4-verify reports **UNVERIFIABLE** (exit 77). That is
  a pre-declared sign-off item, not a defect. **Built and exercised at Check:** the trait, the
  server implementation (every method called by legs 1–2), and the unchanged wire (leg 3).
  **Demonstrated red:** Do makes one server method temporarily answer something other than the
  typed "not implemented", runs leg (1), shows it fail, and puts the output in `build-notes.md`.
- **Production reach:** (a) The real `Gateway` implements the seam, and the test calls it.
  (b) Production never calls it in this slice: routing is untouched, so no request reaches it.
  #668 is the first slice that routes S3 verbs to it. (c) The test calls every method on the
  production implementer, so this is not dead scaffolding.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **Peer to mirror:** `ContainerGateway` (`crates/gateway-core/src/lib.rs:389-431`): the trait
  doc's ADR-0046 / ADR-0010 framing, `Send + Sync + 'static`, `impl Future … + Send` methods, and
  a **default body** so a gateway with no such concept (the S3 crate's own test doubles) needs
  only an empty `impl`. Its server implementation is at `crates/server/src/lib.rs:683-747`. The
  body-carrying method mirrors `ObjectGateway::put_object_streaming`'s streaming signature
  (`crates/gateway-core/src/lib.rs:291-309`) and takes a `ContentHash` (`:245-252`). **Every
  implementer to update:** in `crates/gateway-s3/src/lib.rs`, the `NoGateway` doubles at `:3709` and `:4002`,
  `StoredMetaGateway` at `:4690`, `DefaultRangeSeamGateway` at `:5194` and `VersionSkewGateway` at
  `:5314`; in `crates/server/tests/request_capacity_planes.rs`, `FaultyGateway` at `:191` and
  `MidStreamFaultGateway` at `:260`. The S3 bounds to extend are `:229`, `:797`, `:1527`, `:1638`
  and `:2002`, plus the test helper bound at `:5141`.
- **Prior-art check (triage cycles):** searched by path `crates/gateway-core/src/lib.rs`,
  `crates/gateway-s3/src/lib.rs` and `crates/server/src/lib.rs`. No multipart seam has ever
  merged. `ContainerGateway` (#507, ADR-0046) is the precedent. Rejected: #508's seven monolithic
  attempts, which carried this trait inside a 44-file patch; reviewability was the reason the
  verbs were split into #666–#671. Open PRs on getwyrd/wyrd: none. No PR on #666.
- **Disposition hint:** new-feature

Plan-review response: all three findings checked against `7bf7eb9` and revised in place.
(1) The contract now gives every non-store failure a typed home: an unparsable upload id →
`NoSuchUpload`, an out-of-range part number → a new seam refusal arm, an integrity failure →
`Err(GatewayError::PayloadMismatch)` as on `put_object_streaming`. (2) Leg (2) no longer claims
to prove the S3 bound (assigned to source review, since the workspace has no compile-fail
harness); leg (3) gains a no-call sentinel. (3) `cargo xtask ci` green is now leg (4), with
skipped tool checks recorded as skipped.

## Motivation

The four verb slices (#668–#670, after #667's routing hardening) each need a place to send a
request that is neither HTTP nor a concrete backend. Landing the seam first, empty and typed,
makes each later slice a routing + XML change over a fixed contract. It also keeps the S3 crate
free of `wyrd-core`. Today `gateway-s3` depends only on `wyrd-gateway-core` and `wyrd-traits`
(`crates/gateway-s3/Cargo.toml`), and `gateway-core` only on `wyrd-traits`, `bytes` and
`futures-util` (`crates/gateway-core/Cargo.toml`).

## Design

**The trait.** `pub trait MultipartGateway: Send + Sync + 'static` in `crates/gateway-core/src/lib.rs`,
after `ContainerGateway`, with six methods, one per verb, each taking the container (bucket) name
and the object key as separate strings. The session targets a bucket and an object name, not
the flat `"{bucket}/{key}"`. The methods:
- `create_multipart_upload(container, key, content_type)`.
- `upload_part(container, key, upload_id, part_number, body, expected)`: `body` is a byte
  stream with the same bounds as `put_object_streaming`'s source, and `expected` a `ContentHash`.
  Parts are never buffered (the seam's streaming invariant, `crates/gateway-core/src/lib.rs:289-291`).
- `complete_multipart_upload(container, key, upload_id, named_parts)`: `named_parts` is the
  ordered list of (part number, client ETag) the client sent.
- `abort_multipart_upload(container, key, upload_id)`.
- `list_parts(container, key, upload_id)`.
- `list_multipart_uploads(container)`.

The upload id travels as `&str` and the part number as `u32`. Validating them is the
implementer's job (core's `UploadId` / `PartNumber`, ADR-0045), not the seam's, but **the seam
must give each validation failure a typed answer**, so later slices only replace bodies:
- **An upload id that does not parse** answers the `NoSuchUpload` arm. An id that cannot be
  parsed cannot name a session, and S3 answers it the same way.
- **A part number outside `[1, MAX_PART_NUMBER]`** (core's `PartNumber::new` refuses it with
  `RecordError::PartNumberOutOfRange`, `crates/core/src/multipart.rs:938-942`), in `upload_part`
  or in `complete_multipart_upload`'s named list, answers a **new neutral refusal arm** for an
  out-of-range argument that carries the offending number. Core's `Refusal`
  (`crates/core/src/multipart.rs:4302-4346`) has no such arm, because core only ever sees a
  validated `PartNumber`. This arm is the seam's own addition, beside "not implemented".
- **A body that fails its `ContentHash` check** is `Err(GatewayError::PayloadMismatch)`, exactly
  as `put_object_streaming` answers it today (`crates/server/src/lib.rs:392-396`;
  `crates/gateway-core/src/lib.rs:258-267`). The S3 layer already maps that error.
The wire mapping of these answers stays with #668–#670. The listing
methods return the **complete** set, and the wire layer paginates, exactly as `list_container`
does (ADR-0046 seam decision; #508's open question 2).

**The answers are values, not errors.** Each method returns `Result<…Answer>`. `Err` has the
same meaning it has on `put_object_streaming`: the store failed, the body stream failed, or (for
`upload_part` only) the body failed its integrity check as `GatewayError::PayloadMismatch`. It
is never a protocol outcome. The answer enum carries every protocol outcome. The answers mirror core's
typed outcomes one to one (`crates/core/src/multipart.rs:4380-4452`: `CreateOutcome`,
`UploadPartOutcome`, `CompleteOutcome`, `AbortOutcome`). They share one neutral refusal enum
that mirrors core's `Refusal` and its `Backpressure` / `InvalidPart` (`:4263-4350`) arm for arm,
with plain types in place of core's validated ones, **plus two arms of the seam's own**: the
out-of-range-argument arm above, and **"not implemented"**, the answer a gateway gives when it
does not serve multipart. That arm is what every method of this
slice returns, from the trait's default bodies and from the server's implementation. Like core's
(`crates/core/src/multipart.rs:4256-4259`), none of these enums is `#[non_exhaustive]`, so a new
arm breaks every wire mapping at compile time instead of falling into a `_ =>` arm. Success types
for the list verbs carry what S3's responses need (part number, ETag, size, last-modified for a
part; key, upload id and initiated time for an upload), and later slices may extend them.

**The composition root.** `impl<M, C, Co> MultipartGateway for Gateway<M, C, Co>` in
`crates/server/src/lib.rs`, beside the `ContainerGateway` impl, under the same bounds. Write each
method **explicitly**, returning "not implemented", with a one-line comment naming the slice that
replaces it (#668: create, abort, list parts, list uploads; #669: upload part; #670: complete).
Then a later diff is a body replacement.

**The S3 crate.** Every `G: ObjectGateway + ContainerGateway` bound in
`crates/gateway-s3/src/lib.rs` becomes `G: ObjectGateway + ContainerGateway + MultipartGateway`,
and `S3Gateway::new` therefore requires it. Nothing in the S3 crate calls the trait yet. The
denylist and both guards are untouched.

**Docs currency** (`AGENTS.md:154-158`: a new port is a merge requirement): describe the companion
trait where the living architecture doc describes the S3 gateway's L1 seam
(`docs/design/architecture/05-building-block-view.md:126-137`), stating that it exists and
answers "not implemented" until #668.

## Alternatives considered

- **Six methods on `ObjectGateway`** — widens a deliberately object-only trait; rejected by
  ADR-0046 decision 6 (the #508 design proposal reached the same answer).
- **Reuse `wyrd_core::multipart`'s types in the seam** — would make `gateway-core` and
  `gateway-s3` depend on `wyrd-core`, against ADR-0010's rule that concretes meet only at the
  composition root; rejected. The cost is one mapping in the server, which #668 owns.
- **Refusals as a `GatewayError` variant** — makes protocol answers into errors, which core's
  vocabulary deliberately avoids (`crates/core/src/multipart.rs:4252-4259`: "every answer is a
  value, never 'an error'"); rejected.
- **No default bodies** — forces every test double to spell six methods it will never use;
  `ContainerGateway`'s default is the precedent; rejected.

## Impact & compatibility

No client-visible change: every multipart form still answers `501`. `S3Gateway`'s public bound
widens, which breaks no in-tree caller once the implementers above are updated (the
compiler finds them). There is no persisted-record change and no new dependency.

## Open questions

1. The exact names of the answer types and the refusal enum are Do's, within the shape above.
   The shape is fixed: values not errors (with `Err` meaning what it means on
   `put_object_streaming`), mirror core one to one, plus an out-of-range-argument arm and a
   "not implemented" arm, no `#[non_exhaustive]`. Record the chosen names in `build-notes.md` so #667–#670 cite them.
2. If one of core's refusal arms has no meaning at the seam (for example `ProfileSkew`'s
   `Budget` payload), carry it without the payload rather than dropping the arm. The S3 layer
   still has to answer it.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept or reject the declared compile-only baseline evidence — independent stash/test/restore reproduces E0432 for the absent API, but no pre-fix behavioral assertion executes (`brief.md:90`; `pdca-reviewer-666-evidence/red.log:267`).; C4 Verification (red→green) — Resolve the baseline advisory-gate disposition and obtain a complete CI result — four seam tests pass independently, but frozen CI fails on unchanged TiKV/rustls dependencies and reviewer CI stops at a read-only advisory lock; neither establishes a patch-caused build defect (`gate-logs/C4-ci.log:6472`; `pdca-reviewer-666-evidence/ci.log:3266`).; C5 Causal adequacy — Make malformed HTTP framing fail the new wire oracle — otherwise truncated responses can satisfy the claimed unchanged-wire acceptance test; six concrete false passes were reproduced (`crates/server/tests/multipart_gateway_seam.rs:463`, `crates/server/tests/multipart_gateway_seam.rs:473`; `pdca-reviewer-666-evidence/wire-probe.log:1`).; **An empty Complete list has no typed answer.** `crates/gateway-core/src/lib.rs:714-719` takes `named_parts: Vec<(u32, String)>`. Core refuses an empty list with `RecordError::NoPartsNamed` (`crates/core/src/multipart.rs:4095-4097`). Every seam arm for a bad list carries a part number (`InvalidPart { part_number, .. }` at `crates/gateway-core/src/lib.rs:500`, `PartNumberOutOfRange { part_number }` at `:528`), and the trait doc at `:657-659` rules out `Err` for protocol outcomes. So when #670 handles `complete_multipart_upload("b", "k", <valid id>, vec![])`, it has three options, all bad: make up a part number, return `Err` (a 500 where S3 answers `400 MalformedXML`), or change the trait. The brief requires that every validation failure have a typed answer "so later slices only replace bodies" (brief.md:152-154). The cheapest fix that keeps the brief's fixed shape is a doc precondition on the method: `named_parts` is non-empty, because the wire layer refuses an empty list as malformed before calling. The same edit can also say which arm wins when `upload_part` gets both an unparsable upload id and an out-of-range part number. Today `:661-664` names both arms but gives no order.; **The four blocking T4 findings are real, but they cannot change this test's verdict.** I put a panic inside `dechunk` (`crates/server/tests/multipart_gateway_seam.rs:469`) and it fired in legs 2 and 3. Every 501 here comes back with chunked encoding (HTTP's chunk-by-chunk body framing), so `dechunk` is live code. It accepts a body with no terminal `0\r\n\r\n` chunk, and it treats an unparsable size line as 0 (`:473`, `unwrap_or(0)`). The fixed-length branch (`:463`) never checks Content-Length. So a torn response whose first chunk carries the XML would still pass the status, code and subresource checks at `:390-398`. Leg 3's counter is read separately, so no torn response can flip the property this slice claims. Still, T4 is a gating row. The fix is cheap:; `crates/server/tests/multipart_gateway_seam.rs:463` and `crates/server/tests/multipart_gateway_seam.rs:473`: the new wire helper can report success for malformed or truncated responses. The non-chunked branch ignores the declared `Content-Length`; `dechunk` treats invalid sizes as termination, returns accumulated bytes on truncation, and never requires the chunk delimiters or terminal CRLF. For example, a complete first chunk containing both asserted XML fragments followed by EOF passes the 501 assertions without a terminating zero chunk. Likewise, a short body containing those fragments passes despite a larger declared length. This weakens the new wire regression under the protocol-input rubric. Extract/reuse the framing checks already present in `crates/server/tests/s3_chunk_size_flag.rs:294` (`parse_response`) and `crates/server/tests/s3_chunk_size_flag.rs:357` (`decode_chunked`), and verify that truncated fixed-length bodies and malformed/incomplete chunked bodies fail before the XML assertions.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_666/review-b. Deferred, not addressed here: 5 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_666/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared compile-only baseline for this new API — stash/test/restore reproduces E0432 before assertions execute; a separate injected wrong refusal does fail the behavioral assertion (`brief.md:90`; `pdca-reviewer-666-evidence/red.log:7`; `pdca-reviewer-666-evidence/behavioral-red.log:280`).; C4 Verification (red→green) — Resolve the existing TiKV advisory-gate disposition and obtain the required full CI green — the seven seam tests pass, but independent scanning reproduces the frozen all-features failure in unchanged dependencies; this is not a patch-caused defect (`pdca-reviewer-666-evidence/restored-green.log:19`; `pdca-reviewer-666-evidence/deny-rerun.log:10`; `gate-logs/C4-ci.log:6557`).; Low-priority reuse: `crates/server/tests/multipart_gateway_seam.rs:506` and `crates/server/tests/multipart_gateway_seam.rs:574` add near-copies of the response parser and chunk decoder at `crates/server/tests/s3_chunk_size_flag.rs:294` and `crates/server/tests/s3_chunk_size_flag.rs:357`. The copies already differ in repeated-transfer-encoding rejection, checked chunk-size arithmetic, and trailer validation. Extract a shared test-support parser for these two suites, retaining the new malformed-response fixtures, so framing fixes and regression coverage apply to both callers. This is an advisory cleanup, not a correctness blocker.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 6 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 16 mutants tested in 72s: 10 missed, 6 unviable
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
