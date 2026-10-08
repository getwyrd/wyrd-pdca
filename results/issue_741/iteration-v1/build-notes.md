# Build notes — issue 741 / validate-s3-client-layer

Target: getwyrd/wyrd @ main, built in `$PDCA_WORKTREE` on the integration base
`022d76f` (wave fold of #738, #775, #777, #841 on top of `main`). Line numbers below are
post-patch in that worktree unless marked "base".

## 0. Base check (the brief's first instruction)

`ls crates/validate` → `Cargo.toml src/{access_keys,args,lib,main}.rs tests/cli_surface.rs`;
`grep -n validate Cargo.toml` → `33: "crates/validate",`. #774/#775 are folded
(`fa94483 pdca-integrate: issue_775`). The crate exists pre-patch, so C4-verify's red leg will
compile-fail (UNVERIFIABLE, as the brief's Falsifiability pre-declares). Proceeded.

## 1. What changed, and where

| File | Change |
|---|---|
| `crates/validate/src/client.rs` (new) | `S3Client` (`client.rs:84`, impl `:89-292`): SDK built as the cited peer `crates/server/tests/s3_gateway_cluster.rs:100-115` builds it — `behavior_version_latest`, `region`, `endpoint_url`, static creds, `build_http()` connector, `force_path_style(true)`, `RetryConfig::disabled()`, `StalledStreamProtectionConfig::disabled()` (`:99-119`) — plus a connect timeout. `put_object` (`:145`) streams a caller `Stream` through `SizedBody` (`:371`, `impl Body` `:384`), an `http_body::Body` with an exact size hint that is polled only when hyper asks for a frame and that refuses a short / long / failing source as `S3Error::Body`. `get_object` (`:188`) returns `ObjectBody` unread; `ObjectBody::next_chunk` (`:320`) pulls one piece per call, under a body-idle deadline, and checks bytes received against the declared `Content-Length`. `describe` (`:236`) maps `SdkError` onto `S3Error`. |
| `crates/validate/src/error.rs` (new) | `S3Error` (`error.rs:20`): `Service(ServiceError)`, `Unreadable{status,request_id,detail}`, `Transport`, `Timeout{phase,limit}`, `Request`, `Body`. `ServiceError{status, code: ErrorCode, message, request_id}` (`:45`). `ErrorCode::{Code, NoCodeInBody, NoBody}` (`:60`) answers the brief's Open question 4: "no `<Code>` in a body" and "no body" are separate values. |
| `crates/validate/src/lib.rs` | `pub mod client; pub mod error;` + re-exports (`lib.rs:20-32`); module docs updated (`:5-15`); the stderr note no longer says "the S3 client is not wired yet" (the client now exists; `run` still issues no request) (`:124`). |
| `crates/validate/Cargo.toml` | First `[dependencies]`: `aws-sdk-s3`, `aws-smithy-http-client`, `bytes`, `futures-util`, `http-body`, `tokio` (all `.workspace = true`). `[dev-dependencies]`: `tempfile`, `wyrd-chunkstore-fs`, `wyrd-coordination-mem`, `wyrd-gateway-s3`, `wyrd-metadata-redb`, `wyrd-server`, and `tokio` + `io-util` for the test relay. |
| root `Cargo.toml` | `[workspace.dependencies]`: `wyrd-server = { path = "crates/server" }` (`Cargo.toml:71-73`; nothing else depended on it, so it had no entry); `aws-sdk-s3` + `aws-smithy-http-client` with the ADR-0003 audit note (`:84-100`). |
| `deny.toml`, `deny-all-features.toml` | RUSTSEC-2026-0253 ignore **removed** from both (base `deny.toml:77-86`, base `deny-all-features.toml:105-111`). See §3 — this departs from the brief's literal instruction, on facts the brief did not have. |
| `docs/design/architecture/05-building-block-view.md:253` | Docs-currency: the "S3 client arrives with #741" sentence replaced by what the client layer is. |
| `Cargo.lock` | 14 lines: `wyrd-validate`'s new edges only. No version changed (the aws crates already resolved at 1.148.0 etc. for `wyrd-server`'s dev-deps). |
| `crates/validate/tests/s3_client_roundtrip.rs` (new) | The test (§4). |

No file under `crates/server/` is touched. `serve_s3_role` is not called; the fixture composes
`Gateway::new` + `S3Gateway::serve` as `crates/server/tests/s3_http_wire.rs:56-92` does.

## 2. Things the brief got wrong, or I deviated from, and why

1. **The `lru` advisory is already fixed in the base. The waiver is stale.** The brief (and
   proposal 0017 §Dependencies) assume `aws-sdk-s3` pins `lru ^0.16.3`. On this base,
   `Cargo.lock` has `aws-sdk-s3 1.148.0` and a single `lru 0.18.4`; `aws-sdk-s3-1.144.0`'s
   manifest is the first to take `lru = "0.18.2"` (1.141 and 1.142 take `0.16.3`). Dependabot
   commit `0275cd2 deps: bump aws-sdk-s3 from 1.144.0 to 1.148.0` is on the base.
   `cargo deny --offline check advisories` on the **unpatched** base already printed
   `warning[advisory-not-detected]: … deny.toml:86 "RUSTSEC-2026-0253" no crate matched`.
   The waiver's own REMOVAL TRIGGER ("when a Dependabot aws-sdk-s3 bump lifts `lru` past
   0.18.2 … cargo-deny warns `advisory-not-detected` … delete it then") has fired, and
   `deny-all-features.toml:109-110` says "Remove together with the deny.toml entry when the
   `advisory-not-detected` warning appears there." So I deleted both entries instead of
   rewriting the rationale. Rewriting it to say "an unsound `pop()` now sits in a shipped
   binary" would have been false: no unsound `lru` is in any graph. After the change both
   walls are clean with no warning (`cargo deny --offline check` → `advisories ok, bans ok,
   licenses ok, sources ok`; `cargo deny --offline --all-features --config
   deny-all-features.toml check advisories` → `advisories ok`).
   The maintainer's 2026-08-17 acceptance of the exposure therefore has nothing left to
   cover. Sign-off should still confirm the dependency acceptance itself (the ~76 new
   shipped crates in §5), which is unaffected.
2. **`aws-sdk-s3` floor is `1.144.0`, not the brief's `1.137.0`.** The brief asked for the
   same requirement strings as `crates/server/Cargo.toml:126-129` "so the graph resolves one
   copy". Caret requirements of `1.137.0` and `1.144.0` both resolve to the locked 1.148.0, so
   there is still one copy (verified: `Cargo.lock` gained no second `aws-sdk-s3`). The higher
   floor makes "no resolution of the shipped graph can reach `lru` < 0.18.2" a manifest
   property instead of a lockfile accident, which is what lets the waiver go. Cost of
   following the brief literally instead: the same one-line pin, but a `cargo update -p
   aws-sdk-s3 --precise 1.142.0` would then silently reintroduce the unsound `lru` into a
   shipped binary (the deny wall would catch it, red, with no waiver to explain it).
   `aws-smithy-http-client` uses the brief's `1.1.13`.
3. **Only two aws crates pinned, not four.** The brief listed `aws-smithy-runtime-api` and
   `aws-smithy-types` too. The client does not name either directly (`HttpResponse`,
   `ByteStream`, `SdkError`, `RequestId` are all re-exported by `aws-sdk-s3`), and
   `ByteStream::from_body_1_x` is enabled by `aws-sdk-s3`'s `rt-tokio` feature
   (`rt-tokio = [… "aws-smithy-types/http-body-1-x" …]`). Declaring them would be unused
   dependencies that `cargo machete` (in `cargo xtask ci`) flags.
4. **Deadlines added.** The brief says retries and stalled-stream protection off (done). With
   both off, an endpoint that accepts a connection and goes quiet would hang a call forever,
   which breaks the repo rule "every await on external work is bounded" and proposal 0017 §7
   ("explicit deadlines on connect, on the operation, and on the response-body stream").
   `ClientOptions` (`client.rs:53`, defaults `:65-73`): connect 10 s (SDK `TimeoutConfig`), operation 300 s
   (`tokio::time::timeout` around `send()`; for a PUT it covers the upload, so callers sizing
   large objects must raise it — documented on the field), body-idle 60 s per
   `next_chunk`. Clock: all three run on the tokio runtime clock; each bounds one call, no
   lifecycle spans two clocks (stated in the module docs, `client.rs:13-17`). This is not
   SDK retry or stall protection — a timeout is reported as `S3Error::Timeout{phase,limit}`,
   never retried.
5. **Error classification.** `SdkError::TimeoutError` is mapped to `Timeout{Connect}`: no SDK
   operation timeout is configured, so the only SDK-side timeout that can fire is the connect
   one. `SdkError` is `#[non_exhaustive]`; the wildcard arm reports `Transport` with the SDK's
   full text rather than panicking.

## 3. Streaming oracle, and the mutation runs (criterion 3)

Implemented as the brief specified: a loopback relay (`tests/s3_client_roundtrip.rs`,
`start_relay`/`pump`) between the client and the gateway listener, counting bytes each way,
with an optional hold on the gateway→client half.

- **PUT** — 32 MiB from a generator of 64 KiB pieces, never materialised. When the generator
  is asked for its final piece it records the relay's client→gateway count. Assertion: that
  count `> 1 MiB` (stronger than the brief's `> 0`: the brief's `> 0` would pass for a client
  that sent only its request head and then buffered). Margin: the buffers between generator
  and relay (hyper's write buffer, the client socket send buffer, the relay socket receive
  buffer) are a few MiB at most on loopback; a streaming client has ~20+ MiB on the wire
  by then.
- **GET** — relay forwards the first 1 MiB of the response then holds. Both the
  `get_object` call and the first `next_chunk` are bounded at 15 s with a stated message. The
  test also asserts the relay had forwarded `≤ 1 MiB` when the piece arrived, then releases
  the tail and compares every byte with the generator.

Mutation runs (each applied to `client.rs`, test run, then the file restored from a saved
copy; none shipped):

| Mutation | Result |
|---|---|
| PUT buffer: `body.collect().await` into a `Vec`, then `stream::iter` it | `a_put_body_is_on_the_wire_before_the_generator_finishes` **FAILED**: "when the generator was asked for its final piece the relay had forwarded only 0 request bytes to the gateway (floor 1048576): the client held the 33554432-byte body instead of streaming it". The other 3 tests passed. |
| GET collect-then-rechunk in `get_object` (`output.body.collect()`, re-served as 16 KiB pieces) | `a_get_body_yields_while_the_response_tail_is_withheld` **FAILED** after 15 s: "no GET response within 15s while the relay withheld the response past its first 1048576 bytes: the client is collecting the body before returning it". |
| GET collect lazily on the first `next_chunk` (so `get_object` returns promptly), re-served as 16 KiB pieces | Same test **FAILED** after 15 s: "no body piece within 15s while the relay withheld the response past its first 1048576 bytes: the client is collecting the body before yielding it". |

The "collect, then re-chunk small" shape is exactly the one the brief said a weaker oracle
would miss; both placements of it fail.

## 4. Red → green

Command: `cargo test -p wyrd-validate --test s3_client_roundtrip` (run with a `timeout`
wrapper; the brief names this as the developer harness; the gate is `cargo xtask ci`).

- **Green (with the patch):** `4 passed; 0 failed … finished in 4.63s` (full crate:
  `cli_surface` 19 passed + these 4).
- **Red (patch's `crates/validate/src/**` reverted to base, test kept):** does not compile —
  `error[E0432]: unresolved imports wyrd_validate::ClientOptions, wyrd_validate::ErrorCode,
  wyrd_validate::S3Client, wyrd_validate::S3Error`. This is the criterion-absence red the
  brief pre-declared; C4-verify will score it UNVERIFIABLE (exit 77), which is expected.

### Refute-your-own-test answers

- **(a) Genuine red?** Yes. With the fix reverted the test cannot compile (above). And beyond
  absence, criterion 3 is shown red per direction by the three mutations in §3.
- **(b) Production path?** Yes. The test calls the production `wyrd_validate::S3Client` /
  `ObjectBody` — the same code the binary links — through the real `aws-sdk-s3` stack
  (SigV4, aws-chunked framing, hyper 1 over TCP) into the real `wyrd_gateway_s3::S3Gateway`
  over `wyrd_server::Gateway`. Credentials go through the production `wyrd_validate::resolve`.
  Nothing is mocked; the relay only copies bytes.
- **(c) Fixture includes the fault?** Yes. The PUT leg uses a 32 MiB generated body (larger
  than all buffering in the path) so aggregation is observable; the GET leg actually withholds
  the response tail at the socket; criterion 2 uses credentials the gateway really refuses and
  a port nothing listens on. The gateway is a 256 KiB-chunk composition (not the 8-byte one;
  32 MiB / 256 KiB = 128 chunks), so every object is multi-chunk.

## 5. ADR-0003 §2 three-test dependency audit — `aws-sdk-s3` as a shipped dependency

Accepted by the maintainer in the Plan session on 2026-08-17 (per the brief; this is the
record that decision should leave). Facts from this base:

**Direct normal deps of `wyrd-validate`:** `aws-sdk-s3 1.148.0`, `aws-smithy-http-client
1.4.2`, `bytes`, `futures-util`, `http-body 1`, `tokio`. The last four are already vetted
workspace dependencies of shipped crates and gain a consumer only.

**Test 1 — licence.** Every `aws-*` crate is `Apache-2.0` (checked in each crate's
`Cargo.toml`: `aws-sdk-s3`, `aws-smithy-{http-client,runtime,runtime-api,types,http,checksums}`,
`aws-sigv4`, `aws-runtime`, `aws-credential-types`). The new non-aws transitive crates are
`MIT`, `MIT OR Apache-2.0`, `Apache-2.0 OR MIT`, or `Apache-2.0 OR ISC OR MIT`
(`rustls-native-certs`). No licence new to the allowlist: `cargo deny --offline check
licenses` → `licenses ok`, and no `license-not-encountered` change. This was expected — the
whole tree was already in the graph `cargo deny` walks, as `wyrd-server`'s dev-dependency.

**Test 2 — unsafe posture.** `aws-sdk-s3` itself is `#![forbid(unsafe_code)]`. The smithy
runtime crates contain essentially no `unsafe` on the shipped path: `aws-smithy-types` has one
`from_utf8_unchecked` (`str_bytes.rs:54`, over bytes validated at construction);
`aws-smithy-http-client`'s only `unsafe` is under `src/test_util/` (not compiled without its
test feature). The `unsafe` that does arrive is in well-known leaf crates doing SIMD or
low-level work: `crc-fast` (CRC32/CRC64 SIMD, via `aws-smithy-checksums`), `base64-simd` /
`vsimd` / `outref`, `time`, `uuid`, `zeroize`, `sha1`, `md-5`, `lru`, `regex-lite`,
`xmlparser`, `openssl-probe`. None is a crypto provider: no `ring`, `aws-lc-rs`, `rustls` or
`openssl(-sys)` is in the validator's normal tree (`cargo tree -p wyrd-validate -e normal`).
`rustls-native-certs` + `openssl-probe` are pulled by `aws-smithy-http-client`'s
`default-client` feature for certificate loading only; with no TLS feature they are not used
to open a TLS session.

**Test 3 — transitive surface and maintenance.** `cargo tree -p wyrd-validate -e normal`:
137 distinct crates. Compared with `wyrd-server`'s normal+build tree, **76 are new to a
shipped binary** and 61 are shared. The new ones: the 19 `aws-*`/`aws-smithy-*` crates;
`url` + `idna` + the ICU4X set (`icu_*`, `zerovec*`, `yoke*`, `zerofrom*`, `tinystr`,
`litemap`, `writeable`, `potential_utf`, `zerotrie`, `utf8_iter`, `stable_deref_trait`,
`displaydoc`, `synstructure`); `time` (+`deranged`, `num-conv`, `powerfmt`, `time-core`);
`crc-fast`, `crc32fast`, `md-5`, `sha1`, `generic-array`; `base64-simd`, `vsimd`, `outref`,
`hex`, `bytes-utils`, `ryu`; `regex-lite`, `xmlparser`, `uuid`, `lru` (+`allocator-api2`),
`arc-swap`, `spin`, `zeroize`, `ipnet`, `form_urlencoded`, `pin-utils`, `num-integer`,
`rustversion`, `rustls-native-certs`, `rustls-pki-types`, `openssl-probe`. Maintenance:
`aws-sdk-s3` is AWS's first-party SDK, released on a short cadence and already tracked by
Dependabot in this repo (`0275cd2`); the advisory history the repo had with it
(RUSTSEC-2026-0253 via `lru`) has been fixed upstream and is gone from the graph (§2.1).
**Why this crate:** proposal 0017's claim is "a real client can drive Wyrd"; a hand-rolled
client proves only that our client can drive our server, and the in-tree signer
(`wyrd-gateway-s3::sigv4`) is a `wyrd-*` crate the blackbox guard forbids in the binary.

## 6. Honest limits (Production reach)

- **Plain HTTP only.** Nothing here exercises TLS; the deployed endpoint sits behind an
  operator's TLS terminator. A TLS client waits on the rustls crypto-provider decision
  (`crates/gateway-s3/src/lib.rs:50-57`). The minimal feature set (`default-features =
  false`) deliberately leaves `rustls` and `default-https-client` out.
- **In-process composition is `redb` + `mem` + local FS**, not production `fdb` + `etcd`.
  Adequate for a client-layer criterion, which is about the wire.
- **The 5 GiB memory property is out of scope** (brief; #761). Criterion 3 asserts ordering
  at 32 MiB, not RSS.
- The operation deadline (300 s default) covers a whole PUT upload. A later slice moving
  large objects must size it (or move to an idle-based upload deadline); noted on the field.

## 7. Gates run locally

- `cargo fmt --all --check` → clean.
- `cargo clippy -p wyrd-validate --all-targets -- -D warnings` → clean (one finding,
  `wrong_self_convention` on a method first named `from_sdk`, fixed by renaming to
  `describe`).
- `cargo deny --offline check` and the `--all-features` advisories wall → green, no warnings.
- `cargo xtask ci` (via `engine/xtask.sh ci`) → see §8.

## 8. `cargo xtask ci`

`./engine/xtask.sh ci` in `$PDCA_WORKTREE` → **exit 0, "xtask ci: all checks passed"**.
Steps it ran: typos, docs lint + render check, gitlink / unsafe / **blackbox** guards
("wyrd-validate's normal dependency closure holds no wyrd-* crate (#775)" — with the new
`wyrd-*` dev-dependencies present, so the guard really does tell the two kinds apart), fmt
check, clippy, build, `cargo test --workspace --exclude wyrd-dst` (including
`s3_client_roundtrip`: 4 passed in 4.10 s), cargo-machete, the three `cargo deny` walls,
statics gate, deploy-guard, and the `--cfg madsim` DST clippy + tests.

(An earlier run was interrupted partway when I stopped it to add the GET `Content-Length`
check; its log interleaved with the restart's, so I re-ran once more into a clean log. The
result above is from that clean run, on the final code.)
