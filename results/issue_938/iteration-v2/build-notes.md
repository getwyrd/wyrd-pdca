# Build notes — #938 (743.2), iteration 2: the S3 client verbs smoke sends

## Base

- Built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l2`, HEAD **`93a6f3715e8785a22fd1d1ab40206bcf51b3eef4`**
  (`pdca-integrate: issue_932`), the bundle's `stack-base-tip` on `pdca-integration/main-r42c5348f203d`.
- #853's code is on it, as the brief requires: `582da99 pdca-integrate: issue_853` is an ancestor;
  `crates/validate/src/s3/response.rs`, `crates/validate/src/s3/wire.rs` and
  `crates/validate/tests/s3_client_nonconforming_responses.rs` exist; `roxmltree.workspace = true` is under
  `[dependencies]` (`crates/validate/Cargo.toml:44`). Nothing of #853 was recreated.
- Starting point: `iteration-v1/patch.diff` applied cleanly (`git apply --check`), and its test file was
  byte-identical to the bundle's copy. The sign-off said "keep that design", so this iteration changes only
  what the carry-forward names, plus what the surviving mutants showed.
- `crates/validate/src/lib.rs` is untouched. New types stay reachable as `wyrd_validate::s3::…`.
- `patch.diff` is `git diff HEAD` on that commit (new files added intent-to-add, index restored afterwards).

## The carry-forward, item by item (line numbers are in the patched tree)

### 1. Docs currency (the T4 blocking finding)

`docs/design/architecture/05-building-block-view.md:299` now says, in the validator paragraph:
- the client takes the SDK's S3 Express handling out of endpoint resolution, so each reported request is one
  request on the wire;
- the 64 KiB budget also caps every body the client reads whole itself (HEAD, probe, listing page, delete
  result, non-2xx ranged/conditional GET);
- the `TIME_WAIT` cap counts every request of every verb (was "PUT, GET and DELETE");
- a new passage on the #938 verbs: HEAD, ranged and conditional GET, both listings with their continuation
  rules and the 100-page cap (`TooManyPages`), not-quiet `DeleteObjects` reported row by row, the twelve probes,
  the head kept whole (every header line), the body read by the client so a body failure after a 2xx sits
  beside the 2xx, the 2xx-with-`<Error>` reading, the strict document rules (`<Size>` plain digits), and the
  bucket-name rules (percent-encoded segment, S3 Express suffixes sent plainly, `arn:` names refused unsent);
- `roxmltree` reads "S3 error, listing and delete-result documents".

The same staleness was fixed in the code docs: `crates/validate/src/s3.rs:3-6` (module intro),
`:27-31` (budget bullet), `:252` (the `put_object` `TIME_WAIT` note). No other doc describes the client
(searched `*.md` for `PUT, GET and DELETE`, `BUFFERED_BODY_BUDGET`, `S3Client`).

### 2. The scripted-endpoint legs the sign-off asked for

All in `crates/validate/tests/s3_client_smoke_verbs.rs`:

| Asked for | Test |
|---|---|
| 200 + whole correct success document per `Document` root → `Outcome::Success` | `a_2xx_carrying_the_operations_whole_s3_success_is_a_success` (`:2017`): all 12 probes, the 7 document roots and the 5 headers-only successes (200 or 204), each `Outcome::Success { body }` with the exact bytes sent |
| `<Size>+7</Size>` refused | `a_listing_size_that_is_not_plain_decimal_digits_is_refused` (`:2190`): `+7`, `-7`, ` 7`, `7 `, empty, `7.0`, `0x7`, `u64::MAX + 1` all refused; `u64::MAX` read |
| a `<NextMarker>` that differs from the last key is followed | `list_objects_v1_follows_a_next_marker_and_the_last_key_where_a_page_names_none` (`:1840`): four pages, checks each request's `marker` (the differing `<NextMarker>`, then the last key where none is given, then the last key where it is empty) |
| `ListingError::TooManyPages` reached | `a_listing_still_truncated_after_max_listing_pages_pages_stops_there` (`:1888`): V2 and V1, 100 truncated pages each, exactly 100 requests, `TooManyPages { pages: 100 }` |
| `Head::header_lines` / `Head::fields` return repeated headers | `a_head_reports_every_header_line_a_repeated_one_and_one_that_is_not_text_included` (`:1761`): a repeated header (two cases of name), a non-UTF-8 value, `header()` returning `None` for both, `fields()` equal to every line sent |

Also added, to kill the other v1 survivors (see C5 below): `a_304_that_frames_a_chunked_body_is_refused_only_for_a_probe_that_is_not_a_get_or_head`
(`:2048`, binds `Probe::reads`); the probe-name list in the (B) gateway test (`:745`, binds `Probe::operation`);
`Display` / `Debug` text checks in the (C) test, the later-page test, the ranged and conditional GET tests and
the page-cap test; and refusal-wording checks in (C) and (D) (binds `NotADocument::describe`). A V1 page that
is truncated with nothing to continue after is now refused in a test too (`:1642` test, second half at `:1670`).

### 3. Bucket names: the overstated "sent exactly as given"

What the SDK actually does (checked by resolving endpoints directly with `aws-sdk-s3` 1.152.0's
`DefaultResolver` with this client's parameters, in a throwaway test, since removed):
- a name ending `--x-s3` or `--xa-s3` resolves to an S3 Express endpoint (`backend: S3Express`); with session
  auth on, the SDK first sends its own `CreateSession` (`GET /{name}/?session`); with session auth off, it
  still signs for the `s3express` service;
- a name whose first four characters are `arn:` is refused by the rules (path-style), before anything is sent;
- every other name becomes one path segment, percent-encoded outside URI's unreserved set
  (`endpoint_lib/uri_encode.rs:11-40`).

**Chosen:** a thin wrapper around the SDK's resolver, `WithoutS3Express` (`crates/validate/src/s3.rs:469-494`,
wired at `:184`), that keeps the URL and headers of an endpoint marked `backend: S3Express` and drops its S3
Express auth schemes and properties. With no endpoint auth scheme, the SDK signs with plain SigV4 for `s3`
in the configured region with each operation's own options (no double encoding) — the same signing every
other name gets. `.disable_s3_express_session_auth(true)` (`:188`) is kept as a second guard so no
`CreateSession` is ever sent even if a later SDK marks S3 Express endpoints differently. The test now proves
the `--x-s3` and `--xa-s3` probes reach the real gateway as one request, at `/{name}/`, signed
`.../s3/aws4_request`, and get the gateway's own 400 `InvalidRequest`
(`bucket_probes_send_a_name_s3_forbids_as_given_s3_express_suffixes_included`, `:817`). A space in a name is
covered too: it arrives as `%20`.

**Rejected — the session-auth flag alone (1 line).** It stops the `CreateSession`, but the probe is then
signed for `s3express`, and the Wyrd gateway refuses that scope before it looks at the operation
(`crates/gateway-s3/src/sigv4.rs:422`): the answer is a 403 `AuthorizationHeaderMalformed`, not the
gateway's verdict on CreateBucket. Demonstrated as red 6 below (`Credential=…/us-east-1/s3express/aws4_request`).
The wrapper costs about 25 lines of code over that one line, and it is what makes the claim true.

**Rejected — also sending `arn:` names.** The wrapper would have to build the path-style URL itself on the
SDK's refusal: join `{endpoint path}/` and a percent-encoded name, copying the SDK's encode set
(`endpoint_lib/uri_encode.rs:11-40`, 30 bytes) and its URL join. About 25 more lines that re-implement SDK
logic and could drift from it; and `percent-encoding` is not a direct dependency (adding one is out of scope).
The carry-forward allowed narrowing the claim instead, and 743.3's probe name (`wyrd-validate-probe-`) is not
affected. So the claim is narrowed (`crates/validate/src/s3/verbs.rs:35-43`, the architecture doc) and
`arn:` names are reported as unsent.

**`RequestNotBuilt`, not `NoResponse`.** The SDK reports an endpoint-rule refusal as a dispatch failure,
which `S3Client::classify` reads as `NoResponse`. The capture interceptor now also records
`read_before_transmit` (`crates/validate/src/s3/answer.rs:453-461`, flag `:387`, read at `:427`), the last
hook before the connector is called (`aws-smithy-runtime` 1.16.0 `client/orchestrator.rs:468-500`). In
`captured()` (`crates/validate/src/s3/verbs.rs:400-412`), a `NoResponse` for a call that never reached that
hook becomes `RequestNotBuilt`. With this client's configuration nothing reaches the wire before that hook
(static credentials, endpoint resolution only, no S3 Express session). Red 7 below shows why that last
condition matters: with session auth turned back on, a `CreateSession` *is* sent before the hook, so the
mapping would be wrong there; the production config rules that out.
Tests: `a_bucket_name_the_sdk_reads_as_an_arn_is_refused_before_anything_is_sent` (`:874`; the relay counts
zero connections) and, the other side of the line, `a_probe_that_was_sent_and_got_no_answer_is_no_response_never_unsent`
(`:902`; a probe the endpoint read and then closed on unanswered, and one to a port with no listener, both stay
`NoResponse`). The second was added after the first mutation sweep showed the flag could be dropped or
forced without any test noticing.
Rejected: changing `S3Client::classify` (`crates/validate/src/s3.rs:355`) instead — it is shared with the
existing verbs, and changing their behaviour is out of the brief's scope.

**Effect on the existing verbs.** The resolver and the flag are client-wide, so PUT/GET/DELETE are affected,
but only when the configured bucket itself ends in `--x-s3`/`--xa-s3`: before, they sent a `CreateSession`
first (two requests on the wire, against `s3.rs:3-6`'s promise of one); now they send one plain request. The
carry-forward named this option "on this client". For every other bucket name the resolver returns the SDK's
endpoint unchanged. (E) passes unmodified.

### 4. A 2xx carrying an `<Error>` document

`settle` (`crates/validate/src/s3/answer.rs:343-360`): when a 2xx body is not the operation's success body
but is one whole, non-empty `<Error>` document, the outcome is `Outcome::Error { code, message }`, read by
the same `error_document` rules as any error response; `head.status` stays the 2xx. An empty 2xx body is still
`Unreadable` (an empty body names no error; checked by the test). This applies to every verb that reads a
document (probes, listing pages, delete results), so a 200 + `<Error>` listing page is
`ListingError::Page { cause: Answered(Answer { 200, Outcome::Error }) }`. The `Outcome::Error` doc
(`:139-149`) and `Unreadable` doc (`:150-154`) say so.
Test: `a_200_whose_body_is_an_s3_error_document_keeps_the_200_and_reports_its_code_and_message` (`:2115`).

**Rejected — a new variant (e.g. `Outcome::ErrorInSuccess`).** It would carry the same two fields as
`Outcome::Error`; the only extra fact, "it came with a 2xx", is already in `head.status`. A caller that must
tell them apart reads the status. If 743.1 wants the distinction to be forced by exhaustive matching, that is
a one-variant follow-up; I chose not to grow the enum for a fact the answer already carries.

## Failing gates from iteration 1

- **T4 batch review (gating):** the one finding was docs currency; fixed by item 1.
- **C4 diff coverage:** "patch.diff does not apply on origin/main". Not fixable in the patch: the brief
  requires the #853 integration line as the base, and `run-diff-cov.sh` diffs against `origin/main`. It
  needs the bundle's `stack-base` as its base ref (a harness issue, not a patch defect).
- **C5 surviving mutants:** see "Mutation testing" below.

## Red → green

Verification posture (a) net-new, as the brief pre-declares: with production reverted the test file cannot
build, so C4-verify reports UNVERIFIABLE (77). Green: `cargo test -p wyrd-validate --test s3_client_smoke_verbs`
→ `21 passed; 0 failed` in about 1.7 s; three more runs green while `cargo mutants` loaded the machine.

**(E):** `cargo test -p wyrd-validate` green with the three named files unmodified: `s3_client_roundtrip` 15,
`s3_client_upload_peers` 12, `s3_client_nonconforming_responses` 28 (plus `cli_surface` 21,
`payload_and_sizes` 26, lib unit 4).

**Demonstrated reds.** Each is one edit to production, the test filter run, the file restored and checked
byte for byte (`cmp`), by a helper script in `$PDCA_SCRATCH`. Every one exited 101 with the named test failing.

The five the brief requires:
1. *Only the timeout when a body stalls after a 2xx* (`probe` turns `BodyFailed(Timeout)` into
   `Err(S3Error::Timeout)`) → (C) RED: `22 case(s) lost or misreported the 200`, e.g.
   `CopyObject Stall: the 200 head was lost: Timeout { phase: BodyIdle, limit: 500ms }`.
2. *Accept a listing cut short* (`listing_page` returns an empty final page for a document it cannot read)
   → (D) RED: `6 case(s) not refused as they must be: ListV2 cut-short: not refused as Unreadable: Listed([])`.
3. *Bucket-name handling refuses or rewrites the forbidden name* — rewrite (`trim '-'` + lowercase) → (B) RED
   `left: ("PUT", "/wyrd-validate-probe/")`; refuse (client-side check → `RequestNotBuilt`) → (B) RED
   `CreateBucket "wyrd-validate-probe-": refused or unanswered: the request was not built`.
4. *Stop listing after the first page* → (A) RED against the real gateway: `left: [("list/a", 0), ("list/b", 1)]`.
5. *Drop the `<Error>` rows* → (F) RED: `left: [Deleted { key: "mixed/a" }, Deleted { key: "mixed/c" }]`.

For this iteration's changes:
6. *S3 Express endpoint not stripped, session auth still off* → RED: the request reached the gateway at
   `/wyrd-validate--x-s3/` but signed `Credential=…/us-east-1/s3express/aws4_request`.
7. *Not stripped and session auth on* → RED: the gateway saw a `CreateSession` and answered 403
   `AuthorizationHeaderMalformed`; the probe was never sent.
8. *Pre-transmit failure left as `NoResponse`* → RED: `CreateBucket "arn:aws:s3:::wyrd-validate-probe": not
   refused as unsent: Err(NoResponse { … Path-style addressing cannot be used with ARN buckets … })`.
9. *200 + `<Error>` read as unreadable* (the v1 behaviour) → RED: `13 case(s) lost the 200 or the error`.
10. *An empty 200 read as an error* (guard removed) → RED: `1 case(s) lost the 200 or the error`.
11. *Success root mistyped* (`"CopyObjectResul"`) → RED; *`Document` arm refuses every document* → RED
    (`7 success(es) not reported as one`).
12. *`decimal`: `||` → `&&`* → RED: `<Size>+7</Size>: not refused: Ok(Listing { … size: 7 … })`.
13. *`<NextMarker>` filter inverted* → RED: `left: [None, Some("list/b"), Some("list/c"), Some("")]`.
14. *Page cap + 1* → RED (`Page { page: 101, … }`); *page cap − 1* → RED (the endpoint waits for the 100th
    request; fails after 20 s).
15. *`header_lines` empty* → RED `left: []`; *`fields` empty* → RED `left: []`.
16. *`Probe::reads` → `true`* → RED (`7 case(s) misread`, PUT/POST/DELETE 304s accepted); *→ `false`* → RED
    (`5 case(s) misread`, GET/HEAD 304s refused).
17. *Every unanswered call read as unsent* (`Taken::transmitted` → `false`; `read_before_transmit` no longer
    sets the flag; the match guard forced `true`) → each RED in
    `a_probe_that_was_sent_and_got_no_answer_is_no_response_never_unsent` (the closed-unanswered probe came
    back `RequestNotBuilt`).

Fully reverted (the four modified production files back to HEAD, the test kept): `cargo test -p wyrd-validate
--test s3_client_smoke_verbs` exits 101 with 32 build errors (`no method named probe … list_objects_v2 …
head_object …`) and no `running N tests` line. Files restored and checked with `cmp`.

## Mutation testing (C5, advisory)

Run as the C5 gate runs it (`cargo mutants --in-diff patch.diff --no-shuffle --cap-lints=true`, plus `-j 2`),
on the final `patch.diff`: **124 mutants, 80 caught, 42 unviable, 2 missed** (iteration 1: 111 tested,
54 caught, 17 missed).

How iteration 1's 17 survivors were dealt with:
- `Head::header_lines`, `Head::fields` → empty: killed by the header test.
- `Display` for `Outcome`, `BodyFailure`, `Answer`, `NoResult`, `ListingError`, and `Debug` for `Fetched`:
  killed by the text checks added to (C), the later-page test, the page-cap test, and the ranged and
  conditional GET tests.
- `listing_page` `delete !` (`<NextMarker>` filter): killed by the V1 marker test.
- `decimal` `||` → `&&`: killed by the `<Size>` test.
- `NotADocument::describe` → `"xyzzy"`: killed by the wording checks in (C) and (D).
- `Probe::operation` → `""` / `"xyzzy"`: killed by the probe-name list.
- `Probe::reads` → `true` / `false`: killed by the 304 framing test.
- `Capture::name` → `""` / `"xyzzy"`: **still survive, and are equivalent.** The name is an interceptor's
  label; the SDK shows it only in the error it builds when that interceptor's hook fails, and every `Capture`
  hook returns `Ok`. No input can make the name observable. I did not add them to `.cargo/mutants.toml`'s
  `exclude_re`: that file is repo tooling outside this slice's scope.

The first sweep on this iteration's code also found three new survivors around the "was anything sent" flag
(`Taken::transmitted` → `false`, `read_before_transmit` → `Ok(())`, the match guard → `true`); the
`a_probe_that_was_sent_and_got_no_answer_is_no_response_never_unsent` test was added for them (red 17), and the
final sweep shows them caught.

## Refuting my own test

- **(a) Genuine red? Yes.** Fully reverted, the test does not build (pre-declared UNVERIFIABLE for a net-new
  slice). Partially reverted, each of the seventeen defects above, including the five the brief requires and the
  four carry-forward items, turns a specific assertion red; none stayed green.
- **(b) Production path? Yes.** Every leg drives the production `S3Client`, built through the production
  `resolve_config` and `S3Client::with_deadlines`, on the real `aws-sdk-s3` (its real endpoint resolver,
  wrapped by the production `WithoutS3Express`), the client's own `WireClient`, over real loopback TCP. (A) and
  (B) hit the real `S3Gateway` + `wyrd_server::Gateway` (redb, `MemCoordination`, `FsChunkStore`), with the
  bucket marker seeded as `crates/server/tests/s3_list_objects.rs` does. The relay only forwards bytes, records
  request heads and counts connections. No mock of the client, the SDK or the parser.
- **(c) Fixture includes the fault? Yes.** The faults are on the wire: (C) a real 200 head then a held,
  trickled, cut or unreadable body, and a real 200 + `<Error>` document; (D) documents really cut, trailed,
  doubled, overrun or over budget, and real `+7`-style sizes; the page cap is 100 real truncated pages; the
  repeated and non-UTF-8 headers are real header lines; the S3 Express and ARN names go through the real SDK
  resolver to the real gateway (S3 Express) or to a relay that proves no connection was opened (ARN); the
  "sent, no answer" leg is a real endpoint that reads the request and closes, and a real port with no
  listener.

## Commit-readiness

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-validate --all-targets` (workspace lints, warnings denied): clean.
- `typos`: clean (it flagged a test header value `caf\xe9`; renamed to `value-\xe9`).
- `python3 docs/publishing/tools/lint_docs.py`: OK. `render_site.py --check` (out dir in scratch): link audit OK.
- `cargo xtask ci-dry-run` (every repo-hygiene guard), `cargo xtask blackbox-guard`, `cargo xtask statics`: pass.
  `wyrd-traits` stays a dev-dependency only.
- `cargo doc -p wyrd-validate`: no new warning (the one at `crates/validate/src/sizes.rs:340` is on the base).
- Over-long doc lines (>100 columns) reflowed, including two carried from iteration 1 in `s3/response.rs`.
- I did not run the full `cargo xtask ci` (workspace build/test incl. DST); Check's C4-ci runs it.

## External dependencies

`typos` and `docs-renderer` (`markdown_it` + `yaml`), as the brief lists: both present and run. Nothing
unlisted was needed: no Docker, no network beyond `127.0.0.1`. No NEEDS-HUMAN external-dependency item.
