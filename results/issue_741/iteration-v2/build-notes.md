# Build notes — issue 741 / validate-s3-client-layer (iteration 2)

Target: getwyrd/wyrd @ main, built in `$PDCA_WORKTREE` on the integration base `022d76f`
(wave fold of #738, #775, #777, #841 on top of `main`). Line numbers are post-patch in that
worktree unless marked "base". This iteration starts from the iteration-1 patch
(`iteration-v1/patch.diff`, applied cleanly) and changes what the carry-forward found.

## 0. Base check (the brief's first instruction)

`ls crates/validate` → `Cargo.toml src/{access_keys,args,lib,main}.rs tests/cli_surface.rs`;
`grep -n validate Cargo.toml` → `33: "crates/validate",`. #774/#775 are folded
(`fa94483 pdca-integrate: issue_775`). Proceeded. C4-verify's red leg will compile-fail
(UNVERIFIABLE), as the brief's Falsifiability pre-declares.

## 1. The carry-forward, finding by finding

| # | Finding (iteration 1) | What changed | Evidence |
|---|---|---|---|
| 1 | Empty-body 404 reported as `Code("NotFound")`, a code the SDK makes up (`aws-sdk-s3-1.148.0/src/protocol_serde.rs:24-28`). | `service_error` checks for an empty body **before** it reads the SDK's code (`crates/validate/src/client.rs:323-324`). | `an_empty_error_body_is_no_body_not_a_code_the_sdk_made_up` (`tests/s3_client_roundtrip.rs:657`): a scripted `404, Content-Length: 0` gives `Service{404, NoBody, None, Some(id)}` for GET and DELETE, compared whole with `assert_eq!`. |
| 2 | A 2xx the SDK cannot parse came back as `Service{200, NoBody}` and the SDK's diagnostic was dropped. | A success status reaching the service-error arm is `Unreadable{status, request_id, detail}` with the SDK's full `DisplayErrorContext` (`client.rs:319-321`). | `a_success_the_sdk_cannot_read_is_unreadable_not_an_error_response` (`:753`): `200, Last-Modified: not-a-date` → `Unreadable{200, id, detail}` and the detail names `LastModified`. |
| 3 | Malformed XML / HTML error bodies came back as `Service(NoCodeInBody)` with the diagnostic dropped; no regression told malformed XML from valid XML without `<Code>`. | A non-empty error body must be an S3 `<Error>` document (`is_error_document`, `client.rs:345`, the SDK's own XML reader) or it is `Unreadable`. Of the `<Error>` documents: a `<Code>` the SDK read is the server's; one the SDK read with no `<Code>` is `NoCodeInBody` (the SDK returns that reading as the error's `source`, an `ErrorMetadata`, `client.rs:329`); one the SDK failed partway through is `Unreadable` (`client.rs:330-332`). | `an_xml_error_body_without_a_code_is_told_apart_from_one_that_is_not_xml` (`:681`), three cases: `<Error><Message>…</Message></Error>` → `Service{400, NoCodeInBody, Some(msg), id}` (GET and DELETE); a proxy-style HTML 502 → `Unreadable{502}` (GET and DELETE); `<Error><Code>SlowDown</Code><Message>&bogus;</Message></Error>` → `Unreadable{503}` (its `<Code>` is not reported as read). **First try failed honestly:** without the root check the HTML page came back `NoCodeInBody`, because the SDK's S3 error reader (`rest_xml_unwrapped_errors.rs:35-50`) takes `<Code>` from under any root and its tokenizer accepts the unclosed `<hr>`. That is why the root check exists. |
| 4 | A torn GET body with no declared length (close-delimited) was accepted as a whole, shorter object. | `get_object` refuses a response with no `Content-Length` as `Body{request_id, detail}` (`client.rs:212-219`). `ObjectBody::content_length` is now `u64`, not `Option`. | `a_get_body_with_no_declared_length_is_refused` (`:795`). |
| 5 | Criterion 2's request id was only checked for shape; replacing it with a constant left all tests green. | The relay records the head of the first response on each connection before forwarding it (`pump`, `tests:339-380`), and every request id the tests assert is compared with the `x-amz-request-id` the relay saw the gateway send: the 403 (`:590`), and in criterion 1 each of PUT, GET, DELETE and the not-found GET goes through its own relay (`observed`, `:327`). The scripted endpoint stamps a fixed id that the failure-path tests compare with. | The reviewer's mutation (constant id) now fails `an_error_response_is_a_typed_value…` and `put_get_delete…`. |
| 6 | Root `Cargo.toml` audit said "no rustls", but `default-client` pulls `rustls-native-certs`, `rustls-pki-types`, `openssl-probe`. | Audit note names all three, their licences, and what they do (root `Cargo.toml:99-104`). The "first release" claim for the 1.144.0 floor is now verified: the crates.io index cache shows 1.143.0 takes `lru ^0.16.3`, 1.144.0 is the first with `^0.18.2` (`Cargo.toml:94-97`). | `cargo tree -p wyrd-validate -e normal` lists the three; no `rustls`, `ring`, `aws-lc*`, `openssl(-sys)`. |
| 7 | No tests for deadline expiry, truncated GET, or short / long / failing PUT sources; guard-removal mutants survived. | Tests added: operation deadline (`:866`), body-idle deadline (`:888`), connect deadline (`:918`), GET cut short (`:806`), GET framing that disagrees with its length, both ways (`:820`), unbuildable request (`:960`), PUT source that ends short / runs long in the next piece / runs long within a piece / fails, each followed by a GET proving nothing was stored (`:985`). | `cargo mutants --in-diff`: **75 mutants, 20 caught, 55 unviable, 0 missed** (iteration 1: 14 missed). See §4. |
| 8 | Relay and gateway tasks were detached; fixtures outlived their helpers. | `Owned` guard aborts its task on drop (`tests:107-113`). The relay's and the scripted endpoint's accept loops each hold their per-connection tasks in a `JoinSet`, so aborting the loop aborts every pump/connection task (`:286-324`, `:419-456`). `GatewayFixture` owns the serve task and declares it before the `TempDir`, so the task is aborted before the directory is removed (`:125-157`). | — |
| 9 | C4 diff coverage: "patch.diff does not apply on origin/main". | Not addressable in the patch: the bundle's base is the integration branch (`stack-base` → `pdca-integration/…/main`, with #774/#775 folded), and `crates/validate` does not exist on `origin/main`. The diff-coverage script needs to apply against the stack base. Harness-side. | — |

One part of finding 8 is out of reach, and I'm stating it here rather than hiding it. `S3Gateway::serve` (`crates/gateway-s3/src/lib.rs:210-212`) is `axum::serve`, which spawns each accepted connection as its own task. The fixture can abort the accept loop but cannot own those connection tasks without changing the gateway (out of scope) or bypassing `serve` (the brief says compose through it). They end when their sockets close. Each test declares the gateway fixture first, so its clients and relays drop (closing those sockets) before the fixture does. This is documented on `GatewayFixture` (`tests:120-131`).

## 2. Other changes this iteration, and why

1. **Request ids: one reader for receipts and errors.** Iteration 1 read errors' ids from the
   `x-amz-request-id` header directly but receipts' ids from the SDK's reader, which prefers
   `x-amzn-requestid` when both are present (`aws-types-1.6.0/src/request_id.rs:44-48`). Both
   now use the SDK's reader on the raw response (`RequestId for Response<B>`,
   `client.rs:270`, `:313`). For a server that sends only `x-amz-request-id` (S3, the Wyrd
   gateway) this changes nothing; for one that sends both, a receipt and an error no longer
   name different ids. Stated in the module docs (`client.rs:26-31`).
2. **`S3Error::Body` now carries `request_id`** (`error.rs:48-51`). A GET body failure, or a
   GET refused for having no `Content-Length`, happened after a response arrived. Without the
   id it could not be joined to the server's record (the reason request ids exist here, #529).
   It is `None` for a PUT source fault, where no response was read.
3. **Removed unverifiable code rather than leave equivalent mutants:**
   - `S3Client::options()` and `ErrorCode::as_code()`: unused accessors; their mutants survived
     because nothing called them. The layer above can construct `ClientOptions` itself and
     match on `ErrorCode`.
   - `SizedBody::size_hint`: the SDK frames the aws-chunked upload from the request's
     `Content-Length` header and only falls back to the body's hint when that header is absent
     (`aws-sdk-s3-1.148.0/src/aws_chunked.rs:108-113`). We always set the header, so the hint
     was never read. One declared length now, not two (`client.rs:425-429`).
   - The end-of-body "ended short of the declared length" check in `next_chunk`: unreachable.
     hyper rejects a short `Content-Length`-framed body, and the SDK wraps every GET body in
     its own length check (`aws-smithy-runtime-1.15.0/src/client/http/body/content_length_enforcement.rs:85-95`,
     on by default), which also catches chunked framing that ends short of the declared
     length. Both surface as `Body`. The tests pin that behaviour (`:806`, `:820` first case),
     so an SDK bump that drops the check turns them red.
   - Kept: the **overrun** check (`client.rs:400`). The SDK's check only fires at end of body,
     after the excess bytes would already have reached the caller. The framing test now
     asserts no byte past the declared length is ever handed over. Removing the check by hand
     turns it red: "longer than declared: 3 bytes handed over, past the declared 2".
   - Merged the plain `DispatchFailure(_)` arm into the wildcard (both were `Transport`, an
     equivalent mutant). Removed the `TimeoutError` arm. It cannot arise because no SDK
     operation/attempt timeout is configured (`aws-smithy-runtime-1.15.0/src/client/timeout.rs:91`
     is its only source), so it falls to `Transport`. Comment at `client.rs:280-284`.
4. **`Request` doc corrected by a failing test.** Iteration 1 claimed an endpoint that is not a
   URL is `Request`. My first test for it came back `Transport`: the SDK raises
   `aws_smithy_http::endpoint::ResolveEndpointError` inside dispatch, not construction. I
   rejected downcasting it, because that would add `aws-smithy-http` as a direct dependency
   just to rename one case, and "never reached the endpoint" is what `Transport` means
   anyway. The docs now say that (`error.rs:36-38`), criterion 2's test asserts it, and
   `Request` is tested with an empty key and a `u64::MAX` length (both refused before sending).
5. **New direct dependency `aws-smithy-xml`** (`Cargo.toml:106-109`,
   `crates/validate/Cargo.toml:28-29`) for finding 3. Apache-2.0, same SDK project, already in
   the shipped graph under `aws-sdk-s3` (no new crate, no new licence). No `unsafe` in its
   `src/`, and its one dependency `xmlparser` is `#![forbid(unsafe_code)]`. Rejected
   alternatives:
   - Judging the body by `Content-Type`: a header-based guess that misfiles a server which
     omits it.
   - A hand-rolled root-element check: a new parser where the SDK's own will do, which the
     rubric's "prefer a shared parser" warns against.
   Cost of the chosen route: 3 lines of manifest, 8 lines of code.

## 3. Kept from iteration 1 (reviewed and not re-opened)

- **The RUSTSEC-2026-0253 waiver is deleted, not rewritten** (base `deny.toml:77-86`, base
  `deny-all-features.toml:105-111`). The base lockfile already resolves `aws-sdk-s3` 1.148.0
  and a single `lru` 0.18.4, the advisory matched nothing before this patch, and the waiver's
  own REMOVAL TRIGGER had fired. Iteration 1's reviewer checked this and agreed it is correct,
  and deferred it to sign-off because it departs from the brief's wording. **For sign-off §9:**
  the sentence the brief asks to mirror onto #741 ("the RUSTSEC-2026-0253 exposure was
  accepted") is now false. No unsound `lru` ships. The tracker note should say the exposure
  is moot, while the dependency acceptance itself still stands.
- `aws-sdk-s3` floor `1.144.0` (now verified as the first `lru ^0.18.2` release, §1 row 6),
  `aws-smithy-http-client` `1.1.13`. Neither `aws-smithy-runtime-api` nor `aws-smithy-types`
  is pinned: the client names neither (everything comes through `aws-sdk-s3` re-exports), and
  `cargo machete` would flag them as unused.
- Deadlines (`ClientOptions`, `client.rs:62-80`): connect 10 s, operation 300 s, body-idle
  60 s. All three run on the tokio runtime clock (the SDK's connect timer sleeps on the same
  runtime). Each bounds one call. No lifecycle spans two clocks.

## 4. Streaming oracles, the required mutations, and the mutation run

Oracles unchanged from iteration 1: a loopback relay between the client and the gateway
counts bytes each way and can hold the response tail.

- PUT: 32 MiB generated in 64 KiB pieces. When the generator is asked for its final piece,
  the relay must already have forwarded more than 1 MiB of the request.
- GET: the relay forwards 1 MiB of the response, then holds. A body piece must reach the test
  within 15 s. Then the hold is released and every byte is compared with the generator.

The gateway uses a 256 KiB chunk size, so 32 MiB is 128 chunks (not the 8-byte size of
`s3_http_wire.rs`).

Mutations re-run on **this iteration's** `client.rs`. Each mutant was written to scratch,
swapped in, run, and the file restored from a saved copy. None is shipped.

| Mutation | Result |
|---|---|
| M1 — PUT: `collect()` the caller's stream into a `Vec` before building the body | `a_put_body_is_on_the_wire_before_the_generator_finishes` **FAILED**: "when the generator was asked for its final piece the relay had forwarded only 0 request bytes to the gateway (floor 1048576): the client held the 33554432-byte body instead of streaming it". All other tests passed. |
| M2a — GET: collect the whole body inside `get_object`, then hand it over in 16 KiB pieces | `a_get_body_yields_while_the_response_tail_is_withheld` **FAILED** after 15 s: "no GET response within 15s while the relay withheld the response past its first 1048576 bytes: the client is collecting the body before returning it". |
| M2b — GET: collect lazily on the first `next_chunk`, then hand it over in 16 KiB pieces | Same test **FAILED** after 15 s: "no body piece within 15s … the client is collecting the body before yielding it". |

(Under M2a/M2b three scripted-endpoint tests also failed, because the mutant's own
`collect().expect(..)` panics on a torn body. That is noise from the mutant, not a signal.)

`cargo mutants --in-diff <this diff> --no-shuffle` (the C5 row's command; output kept in
scratch): **75 mutants tested in 2m: 20 caught, 55 unviable, 0 missed.**

## 5. Red → green

Harness: `cargo test -p wyrd-validate --test s3_client_roundtrip` (the brief's named
developer harness; `cargo xtask` has no single-test entry), wrapped in `timeout`.

- **Green:** `16 passed; 0 failed … finished in 4.21s`. Whole crate: `cli_surface` 19 + these 16.
  Six back-to-back runs green (4.06–4.34 s). Four copies of the test binary run at once: all
  exit 0. That load covers the timing-based tests: 300 ms deadlines, the full-backlog connect.
- **Red** (`crates/validate/src/lib.rs` reverted to base so `client`/`error` are not compiled,
  test kept): does not compile — `E0432 unresolved imports wyrd_validate::ClientOptions,
  ErrorCode, Phase, S3Client, S3Error, ServiceError`, `unresolved import
  wyrd_validate::error`. This is the criterion-absence red the brief pre-declares.

### Refute-your-own-test

- **(a) Genuine red? Yes.** With the fix reverted the test cannot compile (above). Beyond
  that absence, there is red by mutation: M1 / M2a / M2b each fail their direction's oracle
  (§4). The overrun check fails when removed (§2.3). `cargo mutants` finds no surviving
  mutant in the diff.
- **(b) Production path? Yes.** Every test calls the production `wyrd_validate::S3Client` /
  `ObjectBody`, the code the binary links. That path runs the real `aws-sdk-s3` stack (SigV4,
  aws-chunked framing, hyper 1 over TCP). Criterion 1 builds its clients with the production
  `S3Client::from_config` over a `ResolvedConfig` from the production `resolve_config`, and
  credentials come through the production `resolve`. The relay only copies bytes. The
  scripted endpoint stands in for the *server*, not the client: it sends raw HTTP responses
  the Wyrd gateway never sends but a validator must classify.
- **(c) Fixture includes the fault? Yes.** Every fault is real at the socket or the wire:
  - The PUT leg is 32 MiB, larger than all buffering in the path.
  - The GET leg really withholds the tail at the socket.
  - Criterion 2 uses a key the gateway really refuses, and a port nothing listens on.
  - Timeouts: the endpoint really never answers; the body really stalls; the connect
    really hangs on a listener with a full accept queue (Linux drops the SYN).
  - Torn and mis-framed bodies are really torn on the wire.
  - PUT sources really end short, run long or fail, against the real gateway, which is then
    shown to hold no object.

The connect-deadline test is `#[cfg(target_os = "linux")]`
(`tests:914-917`). It relies on Linux dropping SYNs to a listener whose accept queue is full;
other kernels may refuse instead. CI and this host are Linux.

## 6. ADR-0003 §2 three-test dependency audit — `aws-sdk-s3` as a shipped dependency

Accepted by the maintainer in the Plan session on 2026-08-17 (per the brief). The numbers below
are from `cargo tree --offline` on this base.

**Direct normal deps of `wyrd-validate`:**
- New: `aws-sdk-s3` 1.148.0, `aws-smithy-http-client` 1.4.2, `aws-smithy-xml` 0.62.1.
- Already vetted workspace dependencies of shipped crates, gaining a consumer only: `bytes`,
  `futures-util`, `http-body` 1, `tokio`.

**Test 1: licence.** All 18 `aws-*` crates in the tree are `Apache-2.0`. Tally of the 75
crates new to a shipped binary:
- 18 `Apache-2.0` (the aws crates)
- 24 `MIT OR Apache-2.0`, 4 `Apache-2.0 OR MIT`, 1 `Apache-2.0/MIT`, 1 `MIT/Apache-2.0`
- 7 `MIT`: `base64-simd`, `generic-array`, `lru`, `outref`, `spin`, `synstructure`, `vsimd`
- 1 `Apache-2.0 OR ISC OR MIT` (`rustls-native-certs`)
- 1 `Apache-2.0 OR BSL-1.0` (`ryu`, satisfied by Apache-2.0)
- 18 `Unicode-3.0`: the ICU4X set under `url`/`idna` — `icu_*`, `zerovec*`, `yoke*`,
  `zerofrom*`, `zerotrie`, `tinystr`, `litemap`, `writeable`, `potential_utf`

`Unicode-3.0` is already on the allowlist (`deny.toml:93`). No licence is new to the allowlist,
because this whole tree was already in the graph `cargo deny` walks, as `wyrd-server`'s
dev-dependency. (Iteration 1's notes left out the `Unicode-3.0` and `BSL` entries. Corrected
here.)

**Test 2: unsafe posture.**
- `aws-sdk-s3` is `#![forbid(unsafe_code)]`.
- `aws-smithy-xml` has no `unsafe` in `src/`, and `xmlparser` is `#![forbid(unsafe_code)]`.
- The smithy runtime crates carry next to none on the shipped path: `aws-smithy-types` has
  one `from_utf8_unchecked` over bytes validated at construction; `aws-smithy-http-client`'s
  `unsafe` is only under `test_util/`.
- The `unsafe` that does arrive sits in well-known leaf crates: `crc-fast` (SIMD CRC via
  `aws-smithy-checksums`), `base64-simd`/`vsimd`/`outref`, `time`, `uuid`, `zeroize`,
  `sha1`, `md-5`, `lru`, `regex-lite`, `openssl-probe`.
- No crypto provider or TLS stack: no `rustls`, `ring`, `aws-lc*`, `openssl(-sys)` in
  `cargo tree -p wyrd-validate -e normal`. `rustls-native-certs`, `rustls-pki-types` and
  `openssl-probe` come with `aws-smithy-http-client`'s `default-client` feature, the only
  feature that builds a hyper 1 connector. They locate and parse the platform's trust store.
  With no TLS feature on, no TLS session is opened.

**Test 3: transitive surface and maintenance.** `cargo tree -p wyrd-validate -e normal`:
137 crates including itself. Against `wyrd-server`'s normal+build tree, **75 are new to a
shipped binary** and 61 are shared. The new ones:
- 18 `aws-*`
- `url` + `idna` + the ICU4X set (`icu_*`, `zerovec*`, `yoke*`, `zerofrom*`, `tinystr`,
  `litemap`, `writeable`, `potential_utf`, `zerotrie`, `utf8_iter`, `stable_deref_trait`,
  `displaydoc`, `synstructure`, `idna_adapter`)
- `time` (+ `deranged`, `num-conv`, `powerfmt`, `time-core`)
- `crc-fast`, `crc32fast`, `md-5`, `sha1`, `generic-array`
- `base64-simd`, `vsimd`, `outref`, `hex`, `bytes-utils`, `ryu`
- `regex-lite`, `xmlparser`, `uuid`, `lru` (+ `allocator-api2`)
- `arc-swap`, `spin`, `zeroize`, `ipnet`, `form_urlencoded`, `pin-utils`, `num-integer`,
  `rustversion`
- `rustls-native-certs`, `rustls-pki-types`, `openssl-probe`

The blackbox guard confirms no `wyrd-*` crate is in that closure.

Maintenance: `aws-sdk-s3` is AWS's first-party SDK on a short release cadence, already
tracked by Dependabot here (`0275cd2`). Its one advisory history in this repo
(RUSTSEC-2026-0253 via `lru`) is fixed upstream and gone from the graph (§3).

**Why this crate:** proposal 0017's claim is "a real client can drive Wyrd". A hand-rolled
client proves only that our client drives our server. The in-tree signer
(`wyrd-gateway-s3::sigv4`) is a `wyrd-*` crate the blackbox guard forbids in the binary.

## 7. Honest limits (Production reach)

- **Plain HTTP only.** Nothing here exercises TLS. The deployed endpoint sits behind an
  operator's TLS terminator. A TLS client waits on the rustls crypto-provider decision
  (`crates/gateway-s3/src/lib.rs:50-57`).
- **The in-process composition is `redb` + `mem` + local FS**, not production `fdb` + `etcd`.
  That is adequate for a client-layer criterion, which is about the wire.
- **The 5 GiB memory property is out of scope** (brief; #761). Criterion 3 asserts ordering at
  32 MiB, not RSS.
- The operation deadline (300 s default) covers a whole PUT upload. A later slice that moves
  large objects must size it (noted on the field, `client.rs:65-66`).
- The relay records only the **first** response head per connection. Each request id the
  tests compare therefore goes through a fresh client + relay pair, one call each, and the
  relay asserts it saw exactly one (`tests:270-283`).

## 8. Gates run locally

- `cargo fmt --all` (applied; `--check` clean), `cargo clippy -p wyrd-validate --all-targets
  -- -D warnings` clean. One finding during the work, `type_complexity` on the PUT-cases
  vector, fixed with a `Pieces` alias (`tests:243`).
- `./engine/xtask.sh ci` (the configured runner, `cargo xtask ci`) on `$PDCA_WORKTREE` with
  the final code: **exit 0, "xtask ci: all checks passed"**. Steps:
  - typos; docs lint and render check
  - gitlink, unsafe and **blackbox** guards. The blackbox guard printed "wyrd-validate's
    normal dependency closure holds no wyrd-* crate (#775)" with the six `wyrd-*`
    dev-dependencies present, so it really does tell the two kinds apart.
  - fmt check, workspace clippy, build
  - `cargo test --workspace --exclude wyrd-dst`, including `s3_client_roundtrip`: 16 passed
    in 4.06 s
  - cargo-machete: no unused dependencies
  - `cargo deny check`: "advisories ok, bans ok, licenses ok, sources ok", no
    `advisory-not-detected` warning. The `--all-features` advisories wall and the licences /
    bans / sources wall are also ok.
  - statics gate, deploy-guard, and the `--cfg madsim` DST clippy and tests

No external dependency was needed beyond the base Rust toolchain (as the brief says): no
Docker, no container, no network. No NEEDS-HUMAN external-dependency item.
