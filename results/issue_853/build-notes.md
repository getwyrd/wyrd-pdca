# Build notes — #853 validate-s3-client-nonconforming-responses

Target: getwyrd/wyrd @ main, stacked on the run's integration branch with #852 folded in:
`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` at **`d9c6225`**
("pdca-integrate: issue_852 … @ 920c43a292de"). All `path:line` below are on that base
(for removed lines) or on the patched tree (for added lines), as marked.

## Step 0 — the base has #852's client

`ls crates/validate/src/` on `d9c6225`: `access_keys.rs args.rs lib.rs main.rs s3 s3.rs`, with
`s3/body.rs` and `s3/error.rs`. The client is `crates/validate/src/s3.rs` (not `client.rs`,
as the brief's ordering note guessed); its error-mapping site is `S3Client::classify`,
base `s3.rs:194-258`. Its `// deferred: #853` markers: base `s3.rs:169-171` (GET framing),
`s3.rs:217-221` (non-conforming error bodies), `s3.rs:247-248` (the `Unreadable` arm),
`s3/body.rs:217-221` (GET length checks). The `// deferred: #854` marker at base
`s3.rs:148-149` is untouched. No STOP.

## Step 1 — baseline on the unchanged base (before any production change)

The test file was written first and built against `d9c6225` unchanged: it compiles (it names
only #852's public API — `resolve_config`, `S3Client::with_deadlines`, `Deadlines`,
`PutSource`, `S3Error`, `ErrorCode`, `BodyError::{Transport{received,..}, LengthUndeclared}`
— and #852's dependencies; no dev-dependency added). Run through
`cargo test -p wyrd-validate --test s3_client_nonconforming_responses` under `timeout`.

73 case/operation pairs. **35 red** (40 failed assertions: each budget case also fails its
bytes-written oracle), **38 green** guards. Per case:

| Case (test fn) | GET | PUT | DELETE | #852's result where red |
|---|---|---|---|---|
| 404 `Content-Length: 0` (bodiless) | green | green | green | — #852 already checks the empty body first (base `s3.rs:222-230`); the brief's repro (`Code("NotFound")`) does **not** reproduce on #852 |
| 503 chunked empty body (bodiless) | green | green | green | — |
| `<Error>` without `<Code>` | **red** | **red** | **red** | `Unreadable` (SDK's "unhandled error") instead of `MissingInXml` |
| HTML 500 / `<!DOCTYPE html>` 500 / plain-text 500 | green ×3 | green ×3 | green ×3 | — #852 already reports `Unreadable` (the SDK finds no code); not reproduced |
| `<Error>` unclosed | **red** | **red** | **red** | `Service{SlowDown}` |
| `<Error>` cut mid-`<Message>` | **red** | **red** | **red** | `Service{SlowDown, "Please reduce your req"}` |
| junk after `</Error>` | **red** | **red** | **red** | `Service{SlowDown}` |
| element after `</Error>` | **red** | **red** | **red** | `Service{SlowDown}` |
| comment after `</Error>` (guard: allowed) | green | green | green | — |
| CL declares more than sent, then close | green | green | green | — hyper's `IncompleteBody` on all three |
| CL declares less than the document | **red** | **red** | **red** | `Service{SlowDown, "Please "}` |
| chunked + CL, agreeing | **red** | **red** | **red** | `Service{SlowDown}` |
| chunked + CL, CL over the data | green | **red** | **red** | GET: the SDK's GET-only length enforcement; PUT/DELETE `Service{SlowDown}` (F3 reproduced) |
| chunked + CL, CL under the data | green | **red** | **red** | same |
| complete XML, chunked, no terminal chunk | green | green | green | — hyper `UnexpectedEof` |
| complete XML, chunked, `0\r\n` then close | green | green | green | — hyper `UnexpectedEof` |
| success the SDK cannot read (GET `Last-Modified`, PUT `x-amz-object-size`, DELETE `x-amz-delete-marker`) | green | **red** | **red** | PUT `Service{200, NoBody}`, DELETE `Service{204, NoBody}`: the SDK's diagnostic dropped |
| budget: 503 chunked, no CL, `<Message>` generated to 69 MB | **red** | **red** | **red** | read whole (69,304,453 body bytes written), `Service{SlowDown, <69 MB message>}` |
| budget: 200 chunked success body generated to 69 MB | — | **red** | **red** | read whole, `Ok` |
| GET 200, CL = 16 × budget + 7 | green | — | — | — |
| GET 200 chunked, 4 × budget + 13, uneven chunks | **red** | — | — | `Body(LengthUndeclared)` |
| GET 200 chunked, empty | **red** | — | — | `Body(LengthUndeclared)` |
| GET 200 close-delimited | green | — | — | — `Body(LengthUndeclared)` |
| GET 200 chunked cut: no terminal / no final CRLF / inside data | green ×3 | — | — | — refused at the head (`LengthUndeclared`) |
| GET 200 CL over sent, then close | green | — | — | — `Transport` |
| GET 200 chunked + CL, CL over / under the data | green ×2 | — | — | — `Body(Length)` |
| GET 200 chunked + CL, agreeing | **red** | — | — | accepted as a 12,293-byte object |

A test bug found on the way and fixed before recording: the generator wrote an empty prefix
as a zero-length chunk, i.e. the terminal chunk, so the success-budget case first looked like
#852 closing early. The generator now skips empty pieces
(`tests/s3_client_nonconforming_responses.rs`, `Body::Generated` loop).

## What changed, and why

Root cause, confirmed in the locked sources: the SDK reads every body it does not stream —
every error, every PUT/DELETE response — whole, before any client code runs
(`aws-smithy-runtime-1.15.0/src/client/orchestrator.rs:526-536`, `read_body` at
`client/orchestrator/http.rs:23-34`); it parses error XML leniently and makes up `NotFound`
(`aws-sdk-s3-1.148.0/src/protocol_serde.rs:24-28`); it enforces `Content-Length` only for GET
(`client/http/body/content_length_enforcement.rs`, `read_before_transmit` keys on
`method() == "GET"`); hyper frames a `Transfer-Encoding` response as chunked and ignores its
`Content-Length` (`hyper-1.10.1/src/proto/h1/role.rs:1286-1299`).

1. **`crates/validate/src/s3/response.rs` (new).** One per-call SDK interceptor,
   `ResponseGuard` (`response.rs:158`), on the one hook that runs after the head and before
   the SDK touches the body, `modify_before_deserialization` (`response.rs:185-227`):
   - `Framing::of` (`response.rs:70-98`) classifies framing from the raw header bytes
     (`get_all_bytes`, so a non-UTF-8 `Transfer-Encoding` is not read as absent): `Length`,
     `Chunked` (exactly `chunked`, case-insensitive), `CloseDelimited`, or `Untrusted`
     (TE with CL; TE other than exactly `chunked`; a CL that is not digits — no `+`/`-` via
     `from_str`, `decimal_length` at `response.rs:104-112`).
   - GET success (status `is_success()`, the SDK's own streaming rule,
     `get_object.rs:337-343`): records the framing in a per-call `OnceLock` slot, body untouched.
   - Every other response (the bodies the SDK buffers): `Untrusted` → error; a declared
     `Content-Length` over the budget → error (allowed by the brief, not relied on);
     otherwise the body is wrapped in `Budgeted` (`response.rs:233-280`), which counts data
     bytes as they arrive and fails at the first piece that crosses the budget, without handing
     it on. The SDK's collect then fails, the hyper body is dropped, and hyper closes the
     connection (`hyper-1.10.1/src/proto/h1/dispatch.rs:230-235`, `conn.rs:849-865`).
     Interceptor errors in this phase become `SdkError::ResponseError` with the raw response
     (`aws-smithy-runtime-api-1.18.0/src/client/orchestrator.rs:193-195`), so #852's existing
     `_` arm reports them as `Unreadable` with status and request id.
   - `BUFFERED_BODY_BUDGET = 64 * 1024` (`response.rs:45`), public, documented.
   - `error_document` (`response.rs:292-341`): the strict reader. Empty → `NoBody`; else UTF-8,
     `roxmltree::Document::parse` (well-formed, no DTD, nothing but Misc after the root),
     root local name `Error`, and after the root only comments and whitespace (a PI is refused,
     per the brief's "anything but whitespace or comments"); `<Code>`/`<Message>` are direct
     children, each at most once (a duplicate is unreadable rather than silently picking one).
2. **`crates/validate/src/s3.rs`.** `put_object` / `delete_object` attach
   `ResponseGuard::buffered()` (`s3.rs:157`, `s3.rs:226`); `get_object` attaches
   `ResponseGuard::streaming_success()` (`s3.rs:178`) and maps the recorded framing
   (`s3.rs:191-211`): `Length(n)` → `ObjectBody` with `Some(n)`; `Chunked` → `None`;
   `CloseDelimited` → `LengthUndeclared` (as #852); `Untrusted` → new `BodyError::Framing`.
   `classify`'s `ServiceError` arm (`s3.rs:252-289`): a 2xx status → `Unreadable` with the
   SDK's `DisplayErrorContext` kept (`s3.rs:258-264`); otherwise the buffered body goes through
   `error_document` (`s3.rs:268`), never the SDK's error metadata. All three `// deferred: #853`
   markers removed (base `s3.rs:169-171`, `:217-221`, `:247-248`); module docs updated
   (`s3.rs:1-28`).
3. **`crates/validate/src/s3/body.rs`.** `ObjectBody.declared: Option<u64>` (`body.rs:173`);
   `content_length() -> Option<u64>` (`body.rs:194`); the end of a chunked body is hyper's
   (`body.rs:223-229` comment, logic `body.rs:233`, `:244`). Marker at base `body.rs:217-221`
   removed. The two `Length` checks are kept as a guard against hyper itself and say so.
4. **`crates/validate/src/s3/error.rs`.** `BodyError::Transport.declared: Option<u64>`
   (`error.rs:100`); new `BodyError::Framing { transfer_encoding, content_length }`
   (`error.rs:114`) carrying the header values as sent; `LengthUndeclared` and `Unreadable`
   docs; `Display` arms.
5. **`crates/validate/Cargo.toml:35-38`.** `roxmltree` moved from `[dev-dependencies]` to
   `[dependencies]` (workspace pin, already ADR-0003-audited with #509; adds `roxmltree` and
   `memchr` to `wyrd-validate`'s normal graph — both already in `Cargo.lock`).
6. **`crates/validate/tests/s3_client_roundtrip.rs:369,1048,1180,1193`.** `Some(len)` for the
   API change (#852's test; its 14 tests still pass against the real gateway, whose error
   bodies are chunked, so the budget and the strict reader run on real gateway errors too).
7. **`docs/design/architecture/05-building-block-view.md:255`.** The validate paragraph now
   states the strict reading, the 64 KiB budget, GET chunked acceptance, and that `roxmltree`
   joins the AWS SDK as a shipped dependency (Docs currency: the client's promise changed and
   the "AWS SDK is the only shipped dependency" sentence would otherwise be false).

### Decisions within the brief's latitude

- **GET 2xx with TE + CL → `BodyError::Framing`, not `Unreadable`.** The brief's GET-success
  family is all "body error" (close-delimited, chunked cut short, length disagreement); a head
  with two framings is a body-level refusal like `LengthUndeclared`. PUT/DELETE/error
  responses with the same headers are `Unreadable` on all three operations, as the brief
  requires. The test asserts only `S3Error::Body(_)` for these GET cases, because the red leg
  must compile against #852, which has no `Framing` variant.
- **A buffered close-delimited body is accepted** (under the budget). It is legal HTTP
  (RFC 9112 §6.3 item 8); for an error document, the strict reader catches a cut anywhere
  inside it. Refusing it would change how a well-formed response is handled (out of scope).
- **A body sent past its `Content-Length`** (CL-only, extra bytes after) is not a case: hyper
  reads exactly CL bytes and the surplus belongs to no response (hyper discards the
  connection when it finds bytes on an idle one). Not observable per request; the
  "declared less than the document" case covers the observable shape (a cut document).
- **`expect` at `s3.rs:194`.** The SDK returns a GET output only after deserialization, which
  runs only after every `modify_before_deserialization` hook has succeeded, and the guard
  sets the slot for every success it passes. An empty slot is a bug in this client, not a peer
  input; no peer bytes reach it.

### Alternatives ruled out (with cost)

- **Header-only budget** (refuse a declared over-budget length): does not bound a chunked body
  with no `Content-Length`; the brief rules it out, and the mutation below proves the test
  catches it.
- **Wrap the HTTP connector instead of an interceptor.** Needs `aws-smithy-runtime-api` as a
  direct dependency (a new pin in the root `Cargo.toml` — an edit outside `crates/validate/`,
  out of scope) plus implementations of `HttpClient` and `HttpConnector` (two traits, the
  `http_connector(&settings, components)` factory and `call()` future) just to reach the same
  response body; the interceptor uses only types `aws-sdk-s3` re-exports
  (`config::Intercept`, `config::interceptors::BeforeDeserializationInterceptorContextMut`,
  `primitives::SdkBody`) and is ~70 lines (`response.rs:156-228`).
- **Keep the SDK's code/message and add a separate well-formedness pass.** Two parsers over the
  same bytes that can disagree (the SDK's reader takes the code from a document roxmltree
  refuses); one strict parse for both the verdict and the values removes that.
- **Hand-rolled XML check.** The rubric prefers a shared parser; `roxmltree` is the
  workspace's audited one (`Cargo.toml:155-163`).
- **Keep `content_length() -> u64` and add a second accessor.** Leaves a `u64` with no meaning
  for a chunked body; `Option<u64>` costs four lines in the roundtrip test.

## Mutation demonstrated (brief: remove the streaming limit, keep the declared-length check)

`response.rs:262` changed to `if false && this.received > BUFFERED_BODY_BUDGET` (the
`DeclaredOverBudget` check at `response.rs:215` kept), then the two budget tests run:

- `an_error_body_with_no_declared_length_is_cut_at_the_budget`: FAILED on GET, PUT, DELETE —
  classification `Service{SlowDown, <69 MB message>}`, and "the endpoint wrote the whole
  69304453-byte body: the client read it to the end instead of closing at its 65536-byte budget".
- `a_put_or_delete_success_body_is_cut_at_the_budget`: FAILED on PUT, DELETE — `Ok(())`, and the
  endpoint wrote the whole 69,304,320 bytes.
- `a_get_object_larger_than_the_budget_is_accepted_whole` stayed green.

Restored afterwards (checked: no `if false` left).

With the fix, bytes the endpoint wrote before it saw the close: GET 213,068; PUT 135,726;
DELETE 147,532 (error case); PUT 147,456, DELETE 147,456 (success case). Stated bound on this
host: 34,652,160 (the table in the test's module docs; dominated by `tcp_rmem[2]` = 32 MiB
here, read from `/proc`, 6 MiB on a default kernel). The generated body is 2 × that bound
(69 MB), so a client that reads it whole exceeds the bound by the bound again.

## Refuting my own test

- **(a) Genuine red? Yes.** With every production file reverted to `d9c6225`
  (`Cargo.toml`, `s3.rs`, `s3/body.rs`, `s3/error.rs`, `s3/response.rs` removed, the roundtrip
  test reverted) and only the new test kept — the same split C4-verify makes — `cargo test
  --quiet -p wyrd-validate --test s3_client_nonconforming_responses` compiled, ran 14 tests,
  and failed 8 by assertion (40 failed assertions, listed above), exit 101. With the fix
  restored: 14/14 pass.
- **(b) Production path? Yes.** The test builds the client exactly as the binary does
  (`resolve_config` → `S3Client::with_deadlines`) and calls `get_object` / `put_object` /
  `delete_object`, so every response passes through the real `aws-sdk-s3` orchestrator, the
  real hyper client over real loopback TCP, the production `ResponseGuard`, `classify`, and
  `ObjectBody`. Nothing is mocked; the only test-side component is the endpoint, which is the
  fault source, not a stand-in for anything the fix changes.
- **(c) Fixture includes the fault? Yes.** The endpoint writes the exact non-conforming bytes
  (unclosed documents, TE + CL, missing terminal chunk, a missing final CRLF, a header that
  fails its grammar) and, for the budget, a body generated lazily to twice the slack bound with
  no `Content-Length`; it counts what it actually wrote until its write fails on the client's
  close. The mutation run shows the oracle firing when the limit is gone.

## Gates

- `cargo fmt -p wyrd-validate`: applied (reflowed two files); clean.
- `cargo clippy -p wyrd-validate --all-targets -- -D warnings`: clean.
- `cargo test -p wyrd-validate`: 47 passed (cli_surface 19, nonconforming 14, roundtrip 14).
- `./engine/xtask.sh ci` (`cargo xtask ci`) in the worktree: **passed** ("xtask ci: all checks
  passed", exit 0). Every stage ran, none skipped: `typos`, the docs render + link audit,
  `cargo fmt --all -- --check`, workspace clippy, the gitlink/unsafe/blackbox guards (the
  blackbox guard confirms `roxmltree` keeps `wyrd-validate`'s normal closure free of `wyrd-*`),
  `cargo deny` (advisories, licences, bans, sources), conformance vectors, the statics and
  deploy guards, the madsim clippy, and the whole test suite including DST.
- The repo configures no other commit hooks (no `.pre-commit-config.yaml`, no
  `core.hooksPath`); `fmt` and clippy above are its commit-time checks.

No external dependency beyond the brief's (none); no NEEDS-HUMAN item.

## For the reviewer-facing side (not in the patch)

- The root `Cargo.toml:85-89` comment on the `aws-sdk-s3` pin counts what #852 added to
  `wyrd-validate`'s shipped graph ("81 crates"); `roxmltree` and `memchr` now join it. That
  comment is outside `crates/validate/` (out of scope here); the 05 doc carries the change.
- `ObjectBody::content_length()` and `BodyError::Transport.declared` changed type
  (`u64` → `Option<u64>`). No caller outside `crates/validate/` exists yet (scenarios arrive
  with #743).
