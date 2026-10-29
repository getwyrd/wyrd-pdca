# Build notes — #853 validate-s3-client-nonconforming-responses (iteration 2)

Target: getwyrd/wyrd @ main, built on the run's stack base
`pdca-integration/main-r42c5348f203d` at **`61e5341`** ("pdca-integrate: issue_744"). That is
`origin/main` `4bac538` (which now has #852 merged as `16f4267` and #854 merged) plus the
folded issue_653 and issue_744. Every `path:line` below is on that base for removed lines, or
on the patched tree for added lines, as marked.

The previous attempt was built on `d9c6225` (#852 folded, #854 not yet). #854 has since
changed the client under this issue (PUTs on their own runtime, `ResponseArrival`, the
whole-body GET deadline), so this is a rebuild on the new base, not a rebase of the old patch.

## Step 0 — the base has #852's client

`ls crates/validate/src/` on `61e5341`: `access_keys.rs args.rs lib.rs main.rs payload.rs s3
s3.rs sizes.rs`, with `s3/body.rs` and `s3/error.rs`. The client is `crates/validate/src/s3.rs`
(not `client.rs`); its error-mapping site is `S3Client::classify`, base `s3.rs:291-354`. Its
`// deferred: #853` markers on the base: `s3.rs:264-266` (GET framing), `s3.rs:313-317`
(non-conforming error bodies), `s3.rs:343-344` (the `Unreadable` arm), `s3/body.rs:442-446`
(GET length checks). All four are removed by this patch. No STOP.

## Step 1 — baseline on the unchanged base (before any production change)

The test was written first and built against `61e5341` unchanged: it compiles. It names only
#852's public API (`resolve_config`, `S3Client::with_deadlines`, `Deadlines` with the `body`
field #854 added, `PutSource`, `S3Error`, `ErrorCode`, `BodyError::Transport { received, .. }`)
and existing dependencies; no dev-dependency added. Run with
`cargo test -p wyrd-validate --test s3_client_nonconforming_responses -- --nocapture` (under
`timeout`; the command the brief names).

Final test against unchanged production code: 19 tests run, **12 fail by assertion**, 7 pass.
**65 case/operation pairs red, 59 green** (a few GET case names are shared by the error suite
and the GET-200 suite, so these counts merge them; each shared name has the same verdict in
both). Per test function (R = red, reproduced; g = green, a guard):

| Test fn / case | GET | PUT | DELETE | Result on the base where red |
|---|---|---|---|---|
| bodiless error: `404` + `Content-Length: 0`; `503` chunked empty | g g | g g | g g | — #852 checks the empty body first. The brief's repro (`Code("NotFound")`) does **not** reproduce on #852 |
| `<Error>` without `<Code>` | R | R | R | `Unreadable` (SDK "unhandled error") instead of `MissingInXml` |
| HTML 500 / doctype 500 / plain text 500 | g | g | g | — not reproduced on #852 |
| unclosed / cut mid-`<Message>` / junk after / element after / **PI after** | R | R | R | `Service{SlowDown}` (cut: message `"Please reduce your req"`) |
| comment after `</Error>` (guard) | g | g | g | — |
| **two `<Code>`s** | R | R | R | `Service{Code("InternalError")}`: the SDK takes the last |
| **element in `<Code>`** / **comment in `<Code>`** | R | R | R | `Service{Code("Slow")}`: the SDK stops at the first markup |
| **element in `<Message>`** | R | R | R | message `"Please "` |
| **CDATA and `&amp;` in `<Message>`** (meant as a guard) | R | R | R | message `"Rate & size "`: the SDK drops the CDATA and what follows. A real defect, reported here, fixed by the strict reader |
| `Content-Length` over sent, then close | g | g | g | — hyper `IncompleteBody` |
| `Content-Length` under the document | R | R | R | `Service{SlowDown, "Please "}` |
| chunked + CL agreeing | R | R | R | `Service{SlowDown}` |
| chunked + CL over / under | g g | R R | R R | GET: the SDK's GET-only length enforcement. PUT/DELETE `Service{SlowDown}` |
| complete XML, chunked, no terminal chunk / no final CRLF | g g | g g | g g | — hyper `UnexpectedEof` |
| **error in TE `gzip`** | R | R | R | `Service{SlowDown}`: read to the close, never decoded |
| **error in TE `chunked<NBSP>`** | g | g | g | — hyper reads framing bytes as the body; the SDK finds no XML |
| success the SDK cannot read | g | R | R | PUT `Service{200, NoBody}`, DELETE `Service{204, NoBody}`: the SDK's diagnostic dropped |
| **`204` + TE chunked + a cut chunk** | g | R | R | PUT/DELETE `Ok(())` |
| **`204` + `Content-Length: 5`** | g | R | R | PUT/DELETE `Ok(())` |
| **`204` + `Content-Length: 0`; `204` with no framing (guards)** | g g | g g | g g | — |
| **budget − 1 / budget, CL and chunked (guards)** | g | g | g | — |
| **budget + 1, CL and chunked** | R | R | R | `Service{SlowDown, <65,5xx-byte message>}` |
| budget: `503` chunked, no CL, `<Message>` generated to 69 MB | R | R | R | read whole (69,304,453 body bytes written), `Service{SlowDown, …}` |
| budget: `200` chunked success body generated to 69 MB | — | R | R | read whole, `Ok` |
| GET `200`, CL = 16 × budget + 7 | g | — | — | — |
| GET `200` chunked, 4 × budget + 13; chunked empty | R R | — | — | `Body(LengthUndeclared)` |
| GET `200` close-delimited | g | — | — | — |
| GET `200` chunked cut: no terminal / no final CRLF / in data | g g g | — | — | — refused at the head (`LengthUndeclared`) |
| **GET TE `chunked<NBSP>` cut; TE `gzip`; TE `gzip, chunked`** | g g g | — | — | — refused at the head (`LengthUndeclared`); these guard against the previous attempt's regression |
| GET CL over sent | g | — | — | — |
| GET chunked + CL over / under / agreeing | g g R | — | — | agreeing: accepted as a 12,293-byte object |

Bold rows are new this iteration.

## The carry-forward, point by point

1. **The test did not pin the 64 KiB budget** (a 16 MiB budget passed; mutants at the budget
   constant and both `>` survived). Added
   `an_error_document_at_the_budget_is_read_and_one_byte_more_is_unreadable`
   (`tests/s3_client_nonconforming_responses.rs:1264`): a valid `SlowDown` document of exactly
   `BUDGET − 1`, `BUDGET` and `BUDGET + 1` bytes (`error_doc_of_len`, `:176`), each with a
   `Content-Length` and chunked in uneven chunks (4093, 1, 8192), on all three operations.
   The first two must be a clean `Service` field by field; the third `Unreadable` whose detail
   names the budget. `BUDGET − 1` and `BUDGET` are both above the ~20 KiB
   `SignatureDoesNotMatch` the reviewer asked for, so that optional case is covered by them.
   Mutation run (below): the constant and the comparison are now killed.
2. **`Transfer-Encoding: chunked\u{00A0}` read as chunked** (Unicode `trim()`). `Framing::of`
   now reads the raw header bytes and trims only SP and HTAB, the optional whitespace of RFC
   9110 §5.6.3 (`ows_trimmed`, `src/s3/response.rs:111-119`); exactly one field line whose
   trimmed bytes equal `chunked` case-insensitively is `Chunked` (`:91`). hyper reads the same
   line as chunked (`hyper` 1.10.1 `headers.rs:130-139`), and anything it cannot read as text
   as not chunked, which this now refuses. Regression: `a_get_in_a_transfer_coding_other_than_
   chunked_is_a_body_error` (`:1517`) — `chunked<NBSP>` cut at 1000 of 3000 bytes, `gzip`, and
   `gzip, chunked` — and `an_error_body_in_a_transfer_coding_other_than_chunked_is_unreadable`
   (`:1106`) on all three operations, whose `gzip` case also pins that the detail names the
   header value as sent.
3. **GET `204` + TE chunked accepted as a 0-byte object.** hyper gives a `204` no body
   whatever its headers say (`hyper` 1.10.1 `proto/h1/role.rs:1268`), and RFC 9112 §6.1 /
   RFC 9110 §8.6 forbid a `204` either framing header. `Framing::of` now returns `Untrusted`
   for a `204` whose framing is anything but none or `Content-Length: 0`
   (`src/s3/response.rs:97-105`). New test `a_204_with_body_framing_is_refused` (`:1200`):
   TE chunked with a cut chunk, and `Content-Length: 5` with 5 bytes, are a body error on GET
   and unreadable on PUT and DELETE (red on the base for PUT and DELETE, which reported `Ok`).
   Guards: `Content-Length: 0` is a success on all three; no framing header is a success on
   PUT and DELETE and stays #852's `LengthUndeclared` on GET.
4. **`<Code>Slow<b/>Down</Code>` read as `SlowDown`.** `error_document` now requires every
   child of `<Code>` and `<Message>` to be a text node (roxmltree text nodes carry CDATA and
   resolved references), else unreadable (`src/s3/response.rs:355-365`). Test
   `a_code_or_message_that_is_not_one_text_field_is_unreadable` (`:931`): element in `<Code>`,
   element in `<Message>`, comment in `<Code>`, two `<Code>`s, and a CDATA/`&amp;` guard.
5. **The "after `</Error>`" rule was enforced but untested; `is_blank` unreachable.** Confirmed
   in the parser: roxmltree skips whitespace in `Misc` after the root and never makes a text
   node of it (`roxmltree` 0.21.1 `tokenizer.rs:279-292`), so `is_blank` is removed and the
   check is `any(|node| !node.is_comment())` (`src/s3/response.rs:340`). Added the
   `pi-after-error` case (`tests/…:870` test fn) next to the existing comment-after guard.
6. **C4 diff coverage: "patch.diff does not apply on origin/main".** The coverage script asks
   `run-verify.sh --print-base` for the base and falls back to `origin/main` only when that
   base is not on origin (`engine/scripts/run-diff-cov.sh:685-690`). This patch is written
   against the stack base `61e5341`. It cannot also apply to bare `origin/main`: it edits the
   `validate` paragraph of `05-building-block-view.md:297` and the `[dependencies]` block of
   `crates/validate/Cargo.toml`, and issue_744 (folded into the base) rewrote that same
   paragraph line and inserted `sha2` next to the same block. If the gate falls back again it
   reports the same; that is a base-resolution question for the gate, not a defect of the patch.

## What changed, and why

Root cause, from the locked sources: the SDK reads every body it does not stream whole before
any client code runs (`aws-smithy-runtime` 1.16.0 `client/orchestrator.rs:526-536`, `read_body`
in `client/orchestrator/http.rs:23-34`); only a GetObject success streams (`aws-sdk-s3`
1.152.0 `operation/get_object.rs:338-341`). It parses error XML leniently and makes up
`NotFound` for a bodiless 404 (`protocol_serde.rs:25-30`). hyper frames a response with any
`Transfer-Encoding` by it and ignores its `Content-Length` (`hyper` 1.10.1
`proto/h1/role.rs:1286-1299`).

1. **`crates/validate/src/s3/response.rs` (new).** One per-call SDK interceptor,
   `ResponseGuard` (`:191`), on the one hook that runs after the head and before the SDK
   touches the body, `modify_before_deserialization` (`:218-257`).
   - `Framing::of` (`:76-107`): `Length`, `Chunked`, `Undeclared`, or `Untrusted` (TE with
     CL; TE other than exactly `chunked`; a CL that is not one decimal length, `decimal_length`
     `:124-141`, digits only with checked arithmetic, as hyper's `headers.rs:45-70`; a `204`
     that frames a body).
   - GET success (the SDK's own streaming rule): records the framing in a per-call `OnceLock`
     slot (`:228-236`), body untouched and uncapped.
   - Every other response: `Untrusted` → refused (`:237-247`); otherwise the body is wrapped in
     `Budgeted` (`:269-308`), which counts data bytes as they arrive and fails at the first
     piece that crosses the budget (`:298`), without handing it on. The SDK's collect drops the
     body at that error (`http.rs:15-21`) and hyper closes the connection. An interceptor
     error here becomes `SdkError::ResponseError` carrying the raw response, so `classify`'s
     existing last arm reports it as `Unreadable` with status and request id.
   - `BUFFERED_BODY_BUDGET = 64 * 1024` (`:45`), public as `wyrd_validate::s3::
     BUFFERED_BODY_BUDGET`, documented with its sizing.
   - `error_document` (`:322-372`): the strict reader. Empty → `NoBody`; else UTF-8,
     `roxmltree::Document::parse` (well-formed, no DTD), root local name `Error`, after the
     root only comments, `<Code>`/`<Message>` direct children at most once each, text only.
2. **`crates/validate/src/s3.rs`.** `put_object` adds `ResponseGuard::buffered()` next to
   #854's `ResponseArrival` (`:248`); `delete_object` adds it (`:322`); `get_object` adds
   `ResponseGuard::streaming_success()` (`:273`) and maps the recorded framing (`:284-306`):
   `Length(n)` → `Some(n)`, `Chunked` → `None`, `Undeclared` → `LengthUndeclared` (as #852),
   `Untrusted` → new `BodyError::Framing`. `classify`'s `ServiceError` arm: a 2xx status →
   `Unreadable` with the SDK's `DisplayErrorContext` kept (`:354-360`); otherwise the body
   goes through `error_document` (`:364`), never the SDK's metadata. The now-unused
   `ProvideErrorMetadata` bound and import are dropped. Module docs: "six things" (`:1-32`).
3. **`crates/validate/src/s3/body.rs`.** `ObjectBody.declared: Option<u64>` (`:381`),
   `content_length() -> Option<u64>` (`:418`); the end of a chunked body is hyper's (comment
   `:454-460`, with the three hyper EOF sites cited); the two length checks apply only with a
   declared length (`:465`, `:476`).
4. **`crates/validate/src/s3/error.rs`.** `BodyError::Transport.declared: Option<u64>`
   (`:118`) with a chunked `Display` arm (`:239`); new `BodyError::Framing` (`:132`) carrying
   both header values as sent; `Unreadable` and `LengthUndeclared` docs updated.
5. **`crates/validate/Cargo.toml:35-39`.** `roxmltree` moved from `[dev-dependencies]` to
   `[dependencies]` (the workspace pin, `Cargo.toml:155-163`, ADR-0003-audited with #509; adds
   `roxmltree` and `memchr` to `wyrd-validate`'s normal graph, both already in `Cargo.lock`).
6. **`crates/validate/tests/s3_client_roundtrip.rs:403,1099,1231,1244`.** `Some(len)` for the
   API change. Its 15 tests run against the real in-process gateway, whose DELETE `204` and
   error bodies now pass through the guard and the strict reader.
7. **`docs/design/architecture/05-building-block-view.md:297`.** The `validate` paragraph now
   states the strict reading, the 64 KiB budget, chunked GET acceptance, and replaces "The AWS
   SDK is the only shipped dependency" (false once `roxmltree` ships) with a sentence that
   holds with or without issue_744's `sha2`.

### Decisions within the brief's latitude

- **GET `2xx` with untrusted framing → `BodyError::Framing`, not `Unreadable`.** The brief's
  GET-success family is all "body error". The test asserts only `S3Error::Body(_)` there,
  because the red leg compiles against #852, which has no `Framing` variant.
- **A `204` with no framing header on GET stays `LengthUndeclared`**, as on #852, though a
  `204` has no body by rule. Accepting it as an empty object would be truer to HTTP but is a
  behavior change the brief does not ask for (S3 never answers a GET with `204`), so it is
  left alone and pinned by a guard.
- **A buffered close-delimited body is accepted** (under the budget): legal HTTP (RFC 9112
  §6.3 item 8), and the strict reader catches a cut anywhere inside an error document.
- **Comments inside `<Code>`/`<Message>` are refused** with elements and PIs. S3 never sends
  them, and the SDK reads such a field only up to the comment (a red above).
- **No header-only budget check.** The first draft also refused a declared `Content-Length`
  over the budget. The mutation run showed it can be deleted with no test noticing, and
  rightly: the counter refuses the same body after reading at most the budget plus one piece.
  It was 6 lines (a `Refusal` variant, its `Display` arm, a match arm) with no classification
  difference, so it is gone. The brief allows a header refusal but does not require one.
- **No `304` special case** (the first draft had one, 5 lines). The client never sends a
  conditional request, so a `304` is not an answer a conforming server gives it; with no
  header budget check, the only difference was whether a `304` with a non-`chunked` coding is
  `Service{304, NoBody}` or `Unreadable`, and no case requires either.
- **`Budgeted` keeps the default `is_end_stream`/`size_hint`.** The first draft forwarded the
  inner body's; four mutants there survived because the SDK's collect reads neither. The
  defaults promise nothing, and an exact hint forwarded from a large declared length would
  invite a consumer to reserve that much.
- **`expect` at `s3.rs:289`.** The SDK returns a GET output only after deserialization, which
  runs only after every `modify_before_deserialization` hook succeeded, and the guard fills the
  slot for every success it passes. An empty slot is a bug in this client, not peer input.

### Alternatives ruled out (with cost)

- **Wrap the HTTP connector instead of an interceptor.** Needs `aws-smithy-runtime-api` as a
  direct dependency (a new pin in the root `Cargo.toml`, outside `crates/validate/`, out of
  scope) plus `HttpClient` and `HttpConnector` implementations (two traits, the
  `http_connector(&settings, components)` factory and `call()`), to reach the same body. The
  interceptor uses only types `aws-sdk-s3` re-exports and is 70 lines (`response.rs:189-258`).
- **Keep the SDK's code/message and add a well-formedness pass.** Two parsers over the same
  bytes that disagree (the baseline shows the SDK taking `Slow` from `Slow<b/>Down`, the last
  of two codes, and dropping CDATA); one strict parse for both verdict and values removes that.
- **Hand-rolled XML check.** The rubric prefers a shared parser; `roxmltree` is the
  workspace's audited one.
- **Keep `content_length() -> u64` and add a second accessor.** Leaves a `u64` with no meaning
  for a chunked body; `Option<u64>` costs four lines in the roundtrip test.

## Mutation demonstrated (brief: remove the streaming limit, keep any declared-length check)

`response.rs:298` changed to `if false && this.received > BUFFERED_BODY_BUDGET`, then the
budget tests run (there is no separate declared-length check to keep; see Decisions):

- `an_error_body_with_no_declared_length_is_cut_at_the_budget`: FAILED on GET, PUT, DELETE —
  `Service{SlowDown, <69 MB message>}`, and "the endpoint wrote the whole 69304453-byte body:
  the client read it to the end instead of closing at its 65536-byte budget".
- `a_put_or_delete_success_body_is_cut_at_the_budget`: FAILED on PUT, DELETE — `Ok(())`, and
  the endpoint wrote the whole 69,304,320 bytes.
- `an_error_document_at_the_budget_is_read_and_one_byte_more_is_unreadable`: FAILED on the
  three `budget-plus-1-chunked` runs; its `Content-Length` runs stayed green (the brief allows a
  refusal from the header), as did `a_get_object_larger_than_the_budget_is_accepted_whole`.

Restored afterwards (checked: no `if false` left). With the fix, bytes the endpoint wrote
before it saw the close: 135,650–201,262 across the five generated-body runs. The stated bound
on this host is 34,652,160, dominated by `tcp_rmem[2]` = 32 MiB; that bound proves the client
stops reading, and the boundary pair (point 1) pins where.

## Mutation gate run locally (`cargo mutants --in-diff`, as `scripts/mutants-in-diff`)

First draft: 82 mutants, 14 missed. Fixes: the header budget check, the `304` arm and the
`is_end_stream`/`size_hint` overrides removed (Decisions); `joined` and `Refusal`'s `Display`
pinned by asserting the detail names the header value (`Transfer-Encoding Some("gzip")`) and
the budget (`"budget"`), strings that appear only in the `Display` text, not the `Debug` dump.
Final: **71 mutants, 40 caught, 2 missed, 27 unviable, 2 timeouts.** The two missed both
replace `ResponseGuard::name` (`response.rs:215`), the label the SDK prints for the
interceptor; nothing depends on its text. The two timeouts replace whole functions
(`delete_object -> Ok(())`, `ObjectBody::read -> Ok(Some(empty))`) and hang a test, which the
gate counts as detected.

## Refuting my own test

- **(a) Genuine red? Yes.** Production files reverted to `61e5341` (`Cargo.toml`, `s3.rs`,
  `s3/body.rs`, `s3/error.rs`, the roundtrip test, the 05 doc; `s3/response.rs` left on disk but
  no longer compiled, as `mod response` is gone), only the new test kept — the split C4-verify
  makes. `cargo test -p wyrd-validate --test s3_client_nonconforming_responses` compiled, ran
  19 tests, failed 12 by assertion (65 red pairs), exit 101. The fix re-applied from the saved
  diff, checked byte-identical with `cmp`: 19/19 pass, 124 pairs green.
- **(b) Production path? Yes.** The client is built as the binary builds it (`resolve_config` →
  `S3Client::with_deadlines`) and the test calls `get_object` / `put_object` / `delete_object`,
  so every response passes through the real `aws-sdk-s3` orchestrator, the real hyper client
  over real loopback TCP, the production `ResponseGuard`, `classify` and `ObjectBody`. Nothing
  is mocked; the only test-side component is the endpoint, which is the fault source.
- **(c) Fixture includes the fault? Yes.** The endpoint writes the exact non-conforming bytes
  (unclosed or trailed documents, nested markup, two codes, TE with CL, `chunked<NBSP>`,
  `gzip`, a `204` with framing, a missing terminal chunk or final CRLF, a header that fails its
  grammar, documents one byte past the budget) and, for the budget, a body generated lazily to
  twice the slack bound with no `Content-Length`, counting what it actually wrote until its
  write fails on the client's close. The mutation runs above show each oracle firing.

## Gates

- `cargo fmt -p wyrd-validate`: applied; clean.
- `cargo clippy -p wyrd-validate --all-targets -- -D warnings`: clean (one
  `cloned_ref_to_slice_refs` in the test fixed with `std::slice::from_ref`).
- `cargo test -p wyrd-validate`: 93 passed (cli_surface 21, payload_and_sizes 26,
  nonconforming 19, roundtrip 15, upload_peers 12).
- `./engine/xtask.sh ci` (`cargo xtask ci`) in the worktree, run twice:
  - **Run 1: exit 1**, one failure, outside this patch:
    `wyrd-gateway-s3 --lib tests::a_bodyless_response_is_recorded_complete_not_aborted`
    (`crates/gateway-s3/src/lib.rs:4666`, "got: " with nothing captured). That crate does not
    depend on `wyrd-validate` and its test binary is the same with or without this patch; the
    test installs a thread-local tracing subscriber (`lib.rs:4626-4635`), and an empty capture
    fits a callsite whose interest another parallel test cached first. It passed 5/5 alone and
    3/3 with its whole lib suite. Every other stage of run 1 passed, DST included. Because
    `cargo test` stops at the first failing binary, run 1 skipped the binaries after it, so
    `cargo test --workspace --exclude wyrd-dst --no-fail-fast` was run next: 207 binaries, 0
    failed.
  - **Run 2: "xtask ci: all checks passed", exit 0.** All 18 stages ran, none skipped:
    `typos`, docs lint and render with link audit, the gitlink/unsafe/blackbox guards (the
    blackbox guard confirms `roxmltree` keeps `wyrd-validate`'s normal closure free of
    `wyrd-*`), `cargo fmt --all -- --check`, workspace clippy, build, the workspace tests (220
    binaries ok, 0 failed), `cargo-machete`, `cargo deny` (advisories, all-features advisories,
    licences, bans, sources), conformance vectors, the statics and deploy guards, the madsim
    clippy and the DST suite.
  - If the gateway test flakes in Check's C4-ci run, it is this pre-existing flake, not the
    patch.
- The repo configures no other commit hooks (no `.pre-commit-config.yaml`, no
  `core.hooksPath`); `fmt` and clippy above are its commit-time checks.

No external dependency beyond the brief's (none); no NEEDS-HUMAN item.

## Left for the human (deferred to sign-off in round 1, not addressed here)

- **A body longer than its declared `Content-Length` is still not detected.** hyper reads
  exactly the declared length and the surplus belongs to no response; seeing it needs the
  socket, which the SDK's connector does not expose. A document whose declared length ends
  inside it is caught (`length-under-sent`); one that ends exactly at `</Error>` with junk
  after is still a clean `Service`, and a GET `200` with `Content-Length: 2000` and 3000 bytes
  sent is still a 2000-byte object. Building a connector-level check or narrowing the brief is
  the deferred scope call.
- **An empty `<Code></Code>` is `Service{Code("")}`.** Whether that should be `MissingInXml`,
  `Unreadable`, or stay is the deferred decision; this patch does not change it.

## For the reviewer-facing side (not in the patch)

- The root `Cargo.toml` comment on the `aws-sdk-s3` pin counts what #852 added to
  `wyrd-validate`'s shipped graph; `roxmltree` and `memchr` now join it. That comment is
  outside `crates/validate/` (out of scope); the 05 doc carries the change.
- `ObjectBody::content_length()` and `BodyError::Transport.declared` change type
  (`u64` → `Option<u64>`). No caller outside `crates/validate/` exists yet (scenarios arrive
  with #743).
