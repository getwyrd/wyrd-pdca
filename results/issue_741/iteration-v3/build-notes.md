# Build notes — issue 741 / validate-s3-client-layer (iteration 3)

Target: getwyrd/wyrd @ main, built in `$PDCA_WORKTREE` on the integration base `022d76f`
(wave fold of #738, #775, #777, #841 on top of `main`). Line numbers are post-patch in that
worktree unless marked "base". This iteration starts from the iteration-2 patch
(`iteration-v2/patch.diff`, applied cleanly) and changes what the carry-forward found.

## 0. Base check (the brief's first instruction)

`ls crates/validate` → `Cargo.toml src tests`; `grep -n validate Cargo.toml` →
`33: "crates/validate",`. #774/#775 are folded (`fa94483 pdca-integrate: issue_775`). Proceeded.
C4-verify's red leg will compile-fail (UNVERIFIABLE), as the brief's Falsifiability
pre-declares, and as iteration 2's reviewer accepted.

## 1. The carry-forward: two blocking defects, each fixed and pinned by a test

Iteration 2's four blocking T4 findings are two defects, each reported twice.

### F1 — a PUT the server acknowledges early was reported as a success

(was `client.rs:184-190`, `:190`.) hyper hands the SDK the response as soon as its head
arrives and keeps polling the body in the background, so the old "check the fault slot once
after `send()`" saw nothing when the server answered before reading the body.

**Fix.** The PUT and its body now share an `Upload` (`crates/validate/src/client.rs:474-485`).
Its `ended: OnceLock<Result<(), String>>` records how the source ended, once:

- `Ok(())` when the source returned `None` with exactly the declared length produced
  (`client.rs:553-556`);
- `Err(detail)` on the first fault (`client.rs:514-518`);
- unset while the source has not ended.

`put_object` gives a receipt only for `Ok` + `Ok(())` (`client.rs:204-227`). An `Ok` from the
SDK with the source not cleanly ended is `S3Error::Body` carrying **that acknowledgement's
request id** (`client.rs:210-216`). The method contract is rewritten to say exactly this
(`client.rs:153-171`), and so is `S3Error::Body` (`crates/validate/src/error.rs:45-50`).

This is the reviewer's suggested fix. It relies on the normal path polling the source to its
`None`: the SDK frames a streaming PUT as aws-chunked, and that framing must see the end of
the source before it can write the terminal chunk and checksum trailer the server waits for.
If that ever stopped being true, every successful PUT in the suite would turn red (the round
trip, the 32 MiB streaming PUT, the new empty-object PUT), so the assumption is pinned, not
trusted.

**Second half of the fix: the source is let go once the PUT has answered.** Reading hyper
showed the reported symptom had a sibling. After the response is handed over, hyper's client
dispatcher keeps polling the request body (`hyper-1.10.1/src/proto/h1/dispatch.rs:381-431`;
the response callback is gone, but `body_rx` stays). That happens on a connection task the SDK
owns. So after `put_object` returned its error, the SDK went on pulling from the caller's
source: a draining server got all 64 MiB uploaded in the background, and a source that never
ends was held forever. The rubric's await rule (helper work stops with its owner) applies.

`StopOnDrop` (`client.rs:487-499`) is held by `put_object` (`client.rs:187`). When the PUT
returns or is dropped, it sets `Upload::stopped` and wakes the body's task through an
`AtomicWaker`. `SizedBody::poll_frame` registers its waker and checks the flag before polling
the source (`client.rs:530-534`). A stopped body fails its next poll, so hyper closes the
connection and drops the source. Documented on the method (`client.rs:169-171`). One limit
stays, and is documented: a connection stuck writing to a server that has stopped reading
is not polled, so it holds the source until that server closes.

**Regression** `a_put_acknowledged_before_its_body_was_sent_is_not_a_receipt`
(`crates/validate/tests/s3_client_roundtrip.rs:1128`). A scripted endpoint reads only the
request head, answers `200 OK` with `Content-Length: 0`, and never reads again. Four cases:

| Case | Expected |
|---|---|
| declared 10, source yields `hello` then never ends | `Body{request_id: Some(id)}`, and the source is dropped after the PUT returns |
| declared 10, `hello`, then fails (after the answer) | `Body{request_id: Some(id)}` |
| declared 64 MiB generator, endpoint answers early **and keeps reading** (`Then::Drain`) | `Body{request_id: Some(id)}`, the generator is dropped, and fewer than 64 MiB were produced (not drained in the background) |
| a `403 AccessDenied` sent on the head alone, source never ends | `Service{403, Code("AccessDenied"), Some("no"), Some(id)}`, and the source is dropped. This is the boundary: an early **refusal** is reported as itself |

"Dropped" is observed with `watched` (`tests:256-271`): the source carries a oneshot sender, so
its receiver resolves when the source is dropped. Each wait is bounded by `within` (20 s). The
"fails later" case is driven by a oneshot the test fires after `put_object` returns, not a
timer, so it cannot race on a slow machine.

**Red, by putting iteration 2's rule back** (R1: treat `Ok` + "not ended" as a receipt,
pattern `None | Some(Ok(()))` on the success arm). Each of the three ack cases, run alone,
returns `Receipt { request_id: Some("scripted-request-741"), e_tag: Some("\"early\"") }`, and
the test panics "an acknowledgement before the body is not a receipt". That is the reviewer's
reproduction exactly. Re-run on the final code: same. Restored, green.

**Red, by removing the stop** (R3a: `StopOnDrop::drop` does nothing):
- the first case fails: "the source released after the PUT returned: no result within 20s";
- the draining 64 MiB case, run alone, fails: "the generator was drained after the PUT had
  answered (67108864 of 67108864 bytes)".

**Red, by keeping the flag but not waking** (R3b): the first case fails the same way, because
a body parked on a pending source is never polled again to see the flag. So the wake is
needed, not decoration. Restored, green.

### F2 — a cut-off or junk-trailed `<Error>` document was read as a complete S3 error

(was `client.rs:345-351`, `:349`.) The old check looked only at the root's start tag, and the
SDK's reader stops at the root's last child, so `<Error><Code>SlowDown</Code>` read as a
complete `SlowDown`.

**Fix.** `not_an_error_document` (`client.rs:387-403`) reads the whole body with `roxmltree`,
which the reviewer pointed at (root `Cargo.toml:169`). The body must be:

- UTF-8;
- well-formed XML 1.0 to the end, with no DTD (roxmltree's default rejects one);
- nothing after the root element but whitespace, comments or processing instructions
  (`roxmltree-0.21.1/src/tokenizer.rs:269-273`);
- rooted at `<Error>`.

Anything else is `Unreadable` (`client.rs:361-365`). The detail now says which rule failed
("not well-formed XML: …", "its root element is <html>, not <Error>") ahead of the SDK's own
text. Before, the detail for these bodies was only the SDK's text, which for a cut-off
document reads as if the parse succeeded.

**Regression** `an_error_document_cut_off_or_followed_by_junk_is_unreadable`
(`tests:789`). The reviewer's three bodies, each with a matching `Content-Length`, on GET and
on DELETE, each `Unreadable` with the scripted status and request id:

- `<Error><Code>SlowDown</Code>` (503)
- `<Error><Code>SlowDown</Code><Message>half` (503)
- `<Error><Code>NoSuchKey</Code></Error><junk` (404)

Plus a positive control: an XML declaration before the document and a comment after it is
still `Service{503, Code("SlowDown"), Some("m"), id}`, so the gate does not over-reject.

**Red, by putting iteration 2's root-tag-only check back** (with `aws-smithy-xml` re-added for
the run). Each body, run alone, fails with the reviewer's exact wrong value:
- `Service{503, Code("SlowDown"), message: None}`
- `Service{503, Code("SlowDown"), message: Some("half")}`
- `Service{404, Code("NoSuchKey"), message: None}`

Restored, green.

### The rest of the carry-forward

- **C5 / T5** asked for regressions for both cases. Done above. The mutation run is in §4.
- **C4-verify posture** was accepted by iteration 2's reviewer. Unchanged.
- **C4 diff coverage, "patch.diff does not apply on origin/main".** Not fixable in the patch.
  The bundle's base is the integration branch (`stack-base` →
  `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`, with #774/#775 folded), and
  `crates/validate` does not exist on `origin/main`. The diff-coverage script needs to apply
  against the stack base. This is on the harness side, as it was in iteration 2.
- **The eight findings deferred to sign-off** (`deferred-findings.json`) are not re-opened
  here. Among them: the RUSTSEC-2026-0253 waiver deletion vs. the brief's "rewrite", and the
  refusal of a complete chunked GET with no `Content-Length`. Both are unchanged from
  iteration 2 and wait for the human.

## 2. Other changes this iteration, and why

1. **A refusal the server sent whole wins over a later source fault**
   (`client.rs:217-219`). With fault-first ordering alone, a server that refuses early (403)
   while the source also fails a moment later gets reported either way, depending on which
   happened before `put_object` looked. Now a fully read error response (`SdkError::ServiceError`)
   is always reported as itself, and a fault wins only when no whole response was read. This is
   one match arm. It removes a race, which is why it is there; **it has no deterministic test**,
   because the race window is between the SDK returning and `put_object`'s check, which a test
   cannot interleave with. The deterministic half (an early refusal with a source that has not
   ended) is the fourth case of the F1 test.
2. **`aws-smithy-xml` is no longer a direct dependency; `roxmltree` is.**
   - Root `Cargo.toml`: the 4-line `aws-smithy-xml` pin and note are gone, and the `roxmltree`
     note gains a 2-line "second consumer" sentence (`Cargo.toml:167-168`).
   - `crates/validate/Cargo.toml:33-36`: `roxmltree.workspace = true`.
   - `roxmltree` 0.21.1 and its one dependency `memchr` are already in the shipped graph, as
     `wyrd-gateway-s3`'s normal dependencies, so this adds no crate and no licence
     (`MIT OR Apache-2.0`; `memchr` is `Unlicense OR MIT`). It was adopted through the ADR-0003
     audit in #509 (root `Cargo.toml:160-169`). `aws-smithy-xml` stays in the graph
     transitively under `aws-sdk-s3`.
3. **The SDK-could-not-read branch keeps a test.** Iteration 2's `&bogus;` body exercised the
   last `else` in `service_error` (`client.rs:370-371`), "a whole `<Error>` document the SDK
   still could not read". roxmltree now rejects `&bogus;` first, so that branch would have
   lost its only test. Added `<Error><Code><b>Slow</b>Down</Code>…</Error>`: well-formed, but
   the SDK's `try_data` fails on markup inside `<Code>`
   (`aws-smithy-xml-0.62.1/src/decode.rs:442-445`). `tests:762-786`, both bodies → `Unreadable{503}`.
4. **The root-name check has a test.** Found by hand-running the mutants cargo-mutants could
   not build (§4): with the guard `has_tag_name("Error")` replaced by `true` (N3), all 19 tests
   still passed. The only non-`<Error>` body in the suite was the sloppy HTML page, which fails
   to parse before the root is ever checked. Added two well-formed bodies with another root
   (`tests:739-764`): an XHTML page, and the wrapped `<ErrorResponse><Error><Code>…` shape other
   AWS protocols use, where the SDK's S3 reader would find no `<Code>` and report
   `NoCodeInBody`. Both → `Unreadable{502}` on GET and DELETE. N3 now fails that test.
5. **`an_empty_object_round_trips`** (`tests:1221`). A 0-byte PUT gives a receipt, and its
   GET gives `content_length() == 0` and no bytes. This guards the new "ended cleanly" rule
   on the one size where a layer might skip polling the source at all. Passed first time.
6. Module docs: `client.rs:26-30` (a success is reported only for what was sent), and the
   `Unreadable` docs (`error.rs:26-31`). The architecture doc paragraph
   (`docs/design/architecture/05-building-block-view.md:253`) now names both behaviours.
   That is the repo's docs-currency rule.

### Considered and not done

- **Take `<Code>`/`<Message>` from roxmltree's tree and drop the SDK's reading.** That would
  remove the double parse and the `ErrorMetadata` downcast (`client.rs:366-369`). It would
  replace the two SDK arms with about 15 lines that walk the root's children. Rejected because
  the brief's Design says the typed error is "constructed from the SDK's structured error
  rather than by re-parsing XML the SDK has already parsed". roxmltree here is only a gate on
  whether the SDK's reading may be believed, and the SDK stays the source of the facts.
- **Detect an early ack by bytes actually written, not by the source ending.** Neither the
  SDK nor hyper exposes what reached the socket. See the limit in §7.

## 3. Kept from iterations 1–2 (reviewed and not re-opened)

- The RUSTSEC-2026-0253 waiver is deleted, not rewritten (base `deny.toml:77-86`, base
  `deny-all-features.toml:105-111`). The base lockfile already resolves `aws-sdk-s3` 1.148.0
  and a single `lru` 0.18.4, so the advisory matched nothing before this patch, and the
  waiver's own REMOVAL TRIGGER had fired. **For sign-off §9:** the sentence the brief asks to
  mirror onto #741 ("the RUSTSEC-2026-0253 exposure was accepted") is now false. No unsound
  `lru` ships. The tracker note should say the exposure is moot, and that the dependency
  acceptance itself still stands. (Deferred to sign-off by iteration 1 and 2's reviewers.)
- `aws-sdk-s3` floor `1.144.0`, the first release that takes `lru ^0.18.2`;
  `aws-smithy-http-client` `1.1.13`.
- Deadlines (`ClientOptions`, `client.rs:63-83`): connect 10 s, operation 300 s, body-idle
  60 s, all on the tokio runtime clock. Each bounds one call; no lifecycle spans two clocks.
- Typed request ids are compared with the header the relay saw the gateway send; scripted
  responses stamp a fixed id the failure-path tests compare with.

## 4. Streaming oracles, the brief's required mutations, and the mutation run

The oracles are unchanged: a loopback relay between the client and the gateway counts bytes
each way and can hold the response tail.

- **PUT:** 32 MiB generated in 64 KiB pieces. When the generator is asked for its final piece,
  the relay must already have forwarded more than 1 MiB of the request.
- **GET:** the relay forwards 1 MiB of the response, then holds. A body piece must reach the
  test within 15 s. Then the hold is released and every byte is compared with the generator.

The gateway uses a 256 KiB chunk size, so 32 MiB is 128 chunks (not the 8-byte size of
`s3_http_wire.rs`).

All mutations ran on **this iteration's** `client.rs`. M1 and R1 were run again on the final
code, after the stop signal was added. M2a and M2b ran before it; the stop signal touches only
the PUT body, not the GET path they mutate. Each mutant was applied in place with the original
saved in scratch, run, and the file restored (a `cmp` against the saved copy confirmed each
restore). None is shipped.

| Mutation | Result |
|---|---|
| M1: PUT, `collect()` the caller's stream into a `Vec` before building the body | `a_put_body_is_on_the_wire_before_the_generator_finishes` **FAILED**: "when the generator was asked for its final piece the relay had forwarded only 0 request bytes to the gateway (floor 1048576): the client held the 33554432-byte body instead of streaming it". The early-ack test also failed, because collecting a source that never ends hangs until the test's 20 s bound. That is the mutant's own hang, not a separate signal. |
| M2a: GET, collect the whole body inside `get_object` | `a_get_body_yields_while_the_response_tail_is_withheld` **FAILED** after 15 s: "no GET response within 15s while the relay withheld the response past its first 1048576 bytes: the client is collecting the body before returning it". |
| M2b: GET, collect lazily on the first `next_chunk` (under the idle deadline), then hand it over in 16 KiB pieces. This is the brief's "collect, then re-chunk small" case. | Same test **FAILED** after 15 s: "no body piece within 15s … the client is collecting the body before yielding it". |
| R1: F1 reverted (iteration 2's PUT success rule) | early-ack test **FAILED**, each of its three ack cases on its own (§1). |
| R2: F2 reverted (iteration 2's root-tag-only check) | cut-off-XML test **FAILED**, each of its three bodies on its own (§1). |
| R3a: no stop (`StopOnDrop::drop` does nothing) | early-ack test **FAILED**: source never released (20 s bound); the draining 64 MiB case alone: "drained … (67108864 of 67108864 bytes)" (§1). |
| R3b: stop flag set, no wake | early-ack test **FAILED**: source never released (§1). |

**`cargo mutants --in-diff patch.diff --no-shuffle`** (the C5 row's command, output kept in
scratch), on the final source: **75 mutants tested in 2m: 19 caught, 56 unviable, 0 missed**
(iteration 2: 75 / 20 caught / 55 unviable / 0 missed). The test file gained two bodies
after this run (§2.4); that adds no mutants, since only source lines are mutated.

"Unviable" hides real mutants here. The workspace lints deny warnings, so a mutant that
replaces a function body with a constant and leaves a parameter or field unread does not
build. Those are exactly the new code's interesting mutants:
- `not_an_error_document → None` / `Some(String::new())` / `Some("xyzzy")`;
- the root-name guard → `true` / `false`;
- `StopOnDrop::drop → ()`.

So I ran them by hand, each written to keep the unread name used (`let _ = body;`, `let _ =
&self.0;`):

| Hand-run mutant | Result |
|---|---|
| N1: `not_an_error_document` → `None` | 2 tests **FAILED** (the XML ones) |
| N2: → `Some(String::new())` | 6 tests **FAILED** (every real gateway error, among others) |
| N3: root guard → `true` | the not-XML test **FAILED**, only because of the two bodies added for it (§2.4); without them, all 19 passed |
| N4: root guard → `false` | 6 tests **FAILED** |
| `StopOnDrop::drop` → nothing | that is R3a above: **FAILED** |

The tool's other unviable mutants are unviable for type reasons
(`put_object → Ok(Default::default())`: `Receipt` has no `Default`).

## 5. Red → green

Runner: `cargo test -p wyrd-validate --test s3_client_roundtrip`, the same command the C4
gate runs (`engine/scripts/run-verify.sh:441`), wrapped in `timeout 600`. The full gate is
`./engine/xtask.sh ci` (§8).

- **Green:** `19 passed; 0 failed … finished in 4.29s` (16 from iteration 2 plus the
  early-ack, cut-off-XML and empty-object tests). Five back-to-back runs green
  (4.05–4.31 s). Four copies of the test binary run at once: all exit 0. That load covers the
  timing-based tests: 300 ms deadlines, the full-backlog connect.
- **Red** (criterion absence): with `crates/validate/src/{client,error}.rs` absent, the test
  does not compile (unresolved `wyrd_validate::{ClientOptions, ErrorCode, Phase, S3Client,
  S3Error, ServiceError}`). That is the red the brief pre-declares, and C4-verify will score
  it UNVERIFIABLE. Re-run this iteration by putting `crates/validate/src/lib.rs` back to base
  (so `client` and `error` are not compiled) and keeping the test: `E0432 unresolved imports
  wyrd_validate::ClientOptions, ErrorCode, Phase, S3Client, S3Error, ServiceError`,
  `unresolved import wyrd_validate::error`, `cannot find type ObjectBody`. The behavioural red
  is by mutation: M1, M2a, M2b, R1, R2, R3a and R3b above.

### Refute-your-own-test

- **(a) Genuine red? Yes.**
  - With the fix reverted, the test cannot compile.
  - Each of this iteration's two fixes, reverted on its own to iteration 2's code, turns its
    regression red, case by case (R1, R2), with the reviewer's reproduced values.
  - Removing the stop signal, or only its wake-up, turns the source-release checks red
    (R3a, R3b).
  - Each streaming direction goes red under its aggregation mutation (M1, M2a, M2b).
- **(b) Production path? Yes.** Every test calls the production `wyrd_validate::S3Client` /
  `ObjectBody`, the code the binary links. That runs the real `aws-sdk-s3` stack: SigV4,
  aws-chunked framing, hyper 1 over TCP. The relay only copies bytes. The scripted endpoint
  stands in for a *server* (it sends raw HTTP the Wyrd gateway never sends), never for the
  client. The XML gate is the production `not_an_error_document`, reached through a real
  error response.
- **(c) Fixture includes the fault? Yes.**
  - The early ack is real on the wire: the endpoint answers after the request head and stops
    reading, so the client's upload really is in flight (the 64 MiB case shows the generator
    mid-stream when the PUT returns).
  - Each malformed error body is really sent, with a `Content-Length` that matches it, so the
    HTTP layer delivers it whole and only the XML is wrong.
  - The streaming legs, timeouts, torn bodies and bad PUT sources are real at the socket, as
    in iteration 2.

## 6. ADR-0003 §2 three-test dependency audit — `aws-sdk-s3` as a shipped dependency

Accepted by the maintainer in the Plan session on 2026-08-17 (per the brief). Numbers are from
`cargo tree --offline -e normal` on this base, compared with `wyrd-server`'s normal+build tree.

**Direct normal deps of `wyrd-validate`** (`cargo tree -p wyrd-validate -e normal --depth 1`):
- New to the shipped graph: `aws-sdk-s3` 1.148.0, `aws-smithy-http-client` 1.4.2.
- Already vetted workspace dependencies of shipped crates, gaining a consumer only: `bytes`,
  `futures-util`, `http-body` 1, `tokio`, and (new this iteration) `roxmltree`.

**Test 1: licence.** All 18 `aws-*` crates are `Apache-2.0`. The 75 crates new to a shipped
binary (by name; recomputed from `cargo metadata` this iteration, same set and tally as
iteration 2):
- 18 `Apache-2.0` (the aws crates)
- 24 `MIT OR Apache-2.0`, 4 `Apache-2.0 OR MIT`, 1 `Apache-2.0/MIT`, 1 `MIT/Apache-2.0`
- 7 `MIT`: `base64-simd`, `generic-array`, `lru`, `outref`, `spin`, `synstructure`, `vsimd`
- 1 `Apache-2.0 OR ISC OR MIT` (`rustls-native-certs`)
- 1 `Apache-2.0 OR BSL-1.0` (`ryu`, satisfied by Apache-2.0)
- 18 `Unicode-3.0`: the ICU4X set under `url`/`idna`

New this iteration, for accuracy: five crates the server already ships also come in at a
second, older version through the SDK:
- `http` 0.2.12 (`MIT OR Apache-2.0`)
- `http-body` 0.4.6 (`MIT`)
- `digest` 0.10.7 (`MIT OR Apache-2.0`)
- `crypto-common` 0.1.7 (`MIT OR Apache-2.0`)
- `foldhash` 0.2.0 (`Zlib`)

Every licence above is already on the allowlist (`deny.toml`, including `Unicode-3.0` and
`Zlib`). No licence is new, because this whole tree was already in the graph `cargo deny`
walks, as `wyrd-server`'s dev-dependency.

**Test 2: unsafe posture.**
- `aws-sdk-s3` is `#![forbid(unsafe_code)]`. `roxmltree` is `#![forbid(unsafe_code)]`.
- The smithy runtime crates carry next to none on the shipped path. `aws-smithy-types` has
  one `from_utf8_unchecked`, over bytes validated at construction. `aws-smithy-http-client`'s
  `unsafe` is only under `test_util/`.
- The `unsafe` that does arrive sits in well-known leaf crates: `crc-fast` (SIMD CRC via
  `aws-smithy-checksums`), `base64-simd`/`vsimd`/`outref`, `time`, `uuid`, `zeroize`, `sha1`,
  `md-5`, `lru`, `regex-lite`, `openssl-probe`.
- No crypto provider or TLS stack: no `rustls`, `ring`, `aws-lc*` or `openssl(-sys)` in
  `cargo tree -p wyrd-validate -e normal`. `rustls-native-certs`, `rustls-pki-types` and
  `openssl-probe` come with `aws-smithy-http-client`'s `default-client` feature, the only
  feature that builds a hyper 1 connector. They find and parse the platform's trust store;
  with no TLS feature on, no TLS session is opened.

**Test 3: transitive surface and maintenance.** `cargo tree -p wyrd-validate -e normal`: 139
crate names including itself. **75 are new to a shipped binary** and 63 are shared (61 in
iteration 2, plus `roxmltree` and `memchr`). The new ones, by group:
- 18 `aws-*`
- `url` + `idna` + the ICU4X set
- `time` and its helpers
- `crc-fast`, `crc32fast`, `md-5`, `sha1`, `generic-array`
- `base64-simd`, `vsimd`, `outref`, `hex`, `bytes-utils`, `ryu`
- `regex-lite`, `xmlparser`, `uuid`, `lru` (+ `allocator-api2`)
- `arc-swap`, `spin`, `zeroize`, `ipnet`, `form_urlencoded`, `pin-utils`, `num-integer`,
  `rustversion`
- `rustls-native-certs`, `rustls-pki-types`, `openssl-probe`

The blackbox guard confirms no `wyrd-*` crate is in that closure (§8).

Maintenance: `aws-sdk-s3` is AWS's first-party SDK on a short release cadence, already tracked
by Dependabot here. Its one advisory history in this repo (RUSTSEC-2026-0253 via `lru`) is
fixed upstream and gone from the graph (§3).

**Why this crate:** proposal 0017's claim is "a real client can drive Wyrd". A hand-rolled
client proves only that our client drives our server. The in-tree signer
(`wyrd-gateway-s3::sigv4`) is a `wyrd-*` crate the blackbox guard forbids in the binary.

## 7. Honest limits (Production reach)

- **Plain HTTP only.** Nothing here exercises TLS. The deployed endpoint sits behind an
  operator's TLS terminator. A TLS client waits on the rustls crypto-provider decision
  (`crates/gateway-s3/src/lib.rs:50-57`).
- **The in-process composition is `redb` + `mem` + local FS**, not production `fdb` + `etcd`.
  That is enough for a client-layer criterion, which is about the wire.
- **The 5 GiB memory property is out of scope** (brief; #761). Criterion 3 asserts ordering
  at 32 MiB, not RSS.
- **Early-ack detection has a floor.** The client sees when its source *ended*, not what the
  server *read*. A server that acknowledges after the source has handed over its last byte
  and ended, but before reading those bytes, is not caught at this layer. With aws-chunked
  framing that means a server answering before the terminal chunk and trailer, and the only
  way to see it is to read the object back. That is the oracle's job (#744), not the client's.
- **The stop signal needs the body to be polled.** After a PUT returns, its source is let go
  on the body's next poll. A connection blocked writing to a server that answered early and
  then stopped reading (without closing) never polls again, so it holds the source until that
  server closes the socket. Fixing that needs a handle on the SDK's connection, which the SDK
  does not expose. No new data is produced from the source in that state.
- The operation deadline (300 s default) covers a whole PUT upload. A later slice that moves
  large objects must size it (noted on the field, `client.rs:68-69`).
- `S3Gateway::serve` spawns each accepted connection as axum's own task, which the test
  fixture cannot own without changing the gateway. Those tasks end when their sockets close;
  each test declares the gateway fixture first, so its clients and relays drop first.

## 8. Gates run locally

- `cargo fmt --all -- --check` clean. `cargo clippy -p wyrd-validate --all-targets -- -D
  warnings` clean. `typos` clean on every touched file.
- `./engine/xtask.sh ci` (the configured runner, `cargo xtask ci`) on `$PDCA_WORKTREE`, run
  three times: after the two fixes; after the stop signal; and on the exact final tree (after
  the two bodies added for N3). All three: **exit 0, "xtask ci: all checks passed"**. Steps:
  - typos; docs lint and render check
  - gitlink, unsafe and **blackbox** guards. The blackbox guard printed "wyrd-validate's
    normal dependency closure holds no wyrd-* crate (#775)" with the six `wyrd-*`
    dev-dependencies present.
  - fmt check, workspace clippy, build
  - `cargo test --workspace --exclude wyrd-dst`, including `s3_client_roundtrip`: 19 passed
    in 4.22 s on the final run
  - cargo-machete: no unused dependencies. So dropping `aws-smithy-xml` left nothing
    dangling, and `roxmltree` is used.
  - `cargo deny check`: "advisories ok, bans ok, licenses ok, sources ok", no
    `advisory-not-detected` warning. The `--all-features` advisories wall and the licences /
    bans / sources wall are also ok.
  - statics gate, deploy-guard, and the `--cfg madsim` DST clippy and tests
- `patch.diff` is `git diff` of the worktree after the third run; `git apply -R --check`
  confirms it matches the tree the gate ran on.

No external dependency was needed beyond the base Rust toolchain, as the brief says: no
Docker, no container, no network. No NEEDS-HUMAN external-dependency item.
