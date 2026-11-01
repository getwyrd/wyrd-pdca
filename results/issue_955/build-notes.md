# Build notes — #955, iteration 3: the S3 client's read verbs and listings (first child of #938)

## Where this starts

- Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`, HEAD **`376b106`** (`main`). Left uncommitted there.
- Salvage hashes checked first, both match the brief (`b50cc4f5…d7d69`, `f2c4fcf9…632f7`). No stop condition.
- The carry-forward asks for fixes, not a new design, so I applied `iteration-v2/patch.diff` (it applies cleanly) and
  changed it. I also read that iteration's `build-notes.md` and its adversary and review reports, because the carry-forward
  quotes them. Earlier notes on what this child keeps from the salvage still hold (`iteration-v1/build-notes.md`,
  `iteration-v2/build-notes.md`).
- `crates/validate/src/lib.rs`, `docs/`, and the three existing client test files are untouched. `Framing::of`
  (`crates/validate/src/s3/response.rs:87`) is untouched, as the sign-off requires.

## The carry-forward, item by item (line numbers are in the patched tree)

**1. Bug: a bodiless answer with a multi-coding `Transfer-Encoding`.** Fixed in the new read path:
`framing_of_no_body` (`crates/validate/src/s3/answer.rs:499`), called from `Capture::read_after_transmit` (`:465`).

The rule: where the answer has no body, a `Transfer-Encoding` alone, of any coding, is not a refusal. "No body" means a `304`
to a GET or HEAD, or any response to a HEAD except a `204`. RFC 9112 §6.1 allows the header on exactly those two, "to indicate
that the origin server would have applied a transfer coding", and hyper gives both no body whatever the headers say
(`hyper` 1.12.0 `proto/h1/role.rs:1301-1305`).

- **`head_object` answered `200` with the same header: fixed, not pinned as intended.** The RFC treats a HEAD and a GET's `304`
  the same way in one sentence, so reporting one as a failed body and not the other would downgrade a conforming answer. To
  know the request was a HEAD, `read_before_transmit` now records the method (`answer.rs:453`). I took it from the request
  rather than adding a constructor argument: `read_after_transmit`'s context does not expose the request
  (`aws-smithy-runtime-api` 1.19.2 `wrappers.rs:194`), and this leaves `Capture::new(bool)` as #956 expects it.
- **Still refused, on purpose, each pinned:** both headers at once (RFC 9112 §6.2 forbids it in any message), a
  `Content-Length` that is no decimal length, and a `204` with a `Transfer-Encoding` even on a HEAD (§6.1 forbids it in any
  response). A `304` to anything but a GET or HEAD stays refused too. No verb in this child can send that, so it has a unit test
  (`answer.rs`, `a_304_to_anything_but_a_get_or_a_head_keeps_its_refusal`).
- **Bytes after such a head are still an overrun.** The test below pins it, for a `304` and for a HEAD's `200`.

Tests in `crates/validate/tests/s3_client_smoke_verbs.rs`:
- `:1647` is the test the sign-off named, extended and renamed from `…frames_a_chunked_body…`. It loops over `chunked`,
  `gzip, chunked`, `gzip`, and the two lines `gzip` + `chunked`, and checks the lines come back as sent.
- `:1694` a HEAD's `200`, `404` and `304`, same four codings.
- `:1737` bytes after the head.
- `:1783` the framing that stays refused.

**2. Listing refusal tests.** New table `SPOILED_PAGES` (`:1144`) and test
`a_listing_page_whose_truncation_or_fields_cannot_be_read_is_refused_never_partial` (`:1258`), through both listing verbs: 19
cases x 2. Each asserts one request sent, status 200, `Unreadable` with the body as sent, and the words of the specific
refusal, so a case cannot pass by being refused for some other reason.
- The four the sign-off lists: no `<IsTruncated>`, `TRUE`, the field twice (in both orders, so neither first-wins nor last-wins
  survives), and markup inside (comment and element).
- Added because the same two functions enforce them: empty and space-padded `<IsTruncated>`; `<NextContinuationToken>`,
  `<NextMarker>`, `<Key>`, `<Size>`, `<ETag>` twice or holding markup; no `<Key>`, no `<Size>`.

**3. Ranged and conditional GET tests.** Both verbs, `206` and `200` respectively:
- `:2075`: no framing gives `Err(LengthUndeclared)`, an untrusted coding and both headers give `Err(Framing)`, status kept.
- `:2130`: a chunked 2xx streams to its terminal chunk (`content_length() == None`, bytes as sent), and one cut before the
  terminal chunk fails as `Transport`.
- `:2182`: a body cut short of its `Content-Length` fails as `Transport` with the status kept.

The sign-off's named mutation, `object_length(framing).unwrap_or(None)` at `crates/validate/src/s3/verbs.rs:273`, now fails
(red CF3a below).

**4. Exact-budget test.** `:1121-1131` compares the full entries, `list/a` and `list/b`, each size 7 with the supplied ETag, for
both listing versions. Red CF4 below is a client that drops ETags: the old `entries.len() == 2` could not see it.

**5. Docs deferral.** `// deferred: #956` at `crates/validate/src/s3/verbs.rs:42`, directly above `head_object`. The
architecture doc is not edited. I did not write `review-rejected.md`; the sign-off asked for the marker. The stale
`review-batch.md` in the bundle is from the previous run and I left it. If the batch reviewer still raises docs currency on the
next run, that rejection is a human call.

## Rejected alternatives

- **Changing `Framing::of`.** The sign-off rules it out: the older verbs share it.
- **Skipping the framing check for bodiless answers altogether, in `read_whole`.** About the same size, but it would also let
  through both headers at once and a `Content-Length` that is no length, which no server may send. That would upgrade an answer
  the client reports today as what it is. It is also why the check keys on `content_length: None`.
- **Pinning the HEAD `200` as intended.** The sign-off allowed it. It would be a smaller diff by about 6 production lines
  (the `head_request` field, its recording, and the `_ => head_request` arm), but it reports a conforming answer as a failed
  body, against the brief's Invariant.

## Running it

The project's runner is `cargo xtask` through `engine/xtask.sh`; it has no single-test subcommand. For the quick passes I used
the command the brief's Falsifiability line names, `cargo test -p wyrd-validate --test s3_client_smoke_verbs`, always under
`timeout`. Check's gates run the real `cargo xtask ci`.

**Green:** `cargo test -p wyrd-validate`: lib 7, `cli_surface` 21, `payload_and_sizes` 26,
`s3_client_nonconforming_responses` 28, `s3_client_roundtrip` 15, `s3_client_smoke_verbs` 28, `s3_client_upload_peers` 12. All
passed. That is (E): the three named files pass unmodified.

## Red → green

Verification posture (a) net-new, as the brief pre-declares: the test cannot build against the base, so C4-verify reports
UNVERIFIABLE (exit 77). Iteration 2 showed the full revert (exit 101, 34 errors); nothing about that has changed, and I did not
re-run it this iteration.

**Single-edit reds, all re-run this iteration on this code.** Each is one edit to production and a run of the lib tests plus the
test file, then the file restored (the script compared hashes afterwards: `RESTORED`). Every run below compiled and exited 101.
Three of my first edits (R2, CF1c, CF1d) did not compile because `warnings = "deny"` caught an unused item. I corrected the
edits and re-ran; the results below are the compiled runs.

Required by the brief:
- **(1)** a 2xx body timeout returned as only the timeout → (C) RED: `2 case(s) lost or misreported the 200`.
- **(2)** accept a cut listing → (D), the later-page test and (C) RED: `8 case(s) not refused as they must be`.
- **(4)** stop after the first page, V2 and V1 separately → (A) RED each time: `assertion failed: ListObjectsV2` /
  `ListObjects`.
- **(15)** every error reads as `NoBody` → (H) RED: `20 case(s) lost the error's status, headers, code or message`; the checksum
  test too (`4 case(s)`).
- **(G)'s 304** refused by its framing (`Capture::new(false)` in `get_object_if_none_match`) → the `304` test RED on
  `["chunked"]`, and the overrun test.

For this iteration's changes:
- **CF1** the fix off (`framing_of_no_body` changes nothing) → RED in four: the `304` test on `["gzip, chunked"]`, the HEAD
  test on `200 OK ["gzip, chunked"]`, the overrun test, the unit test. This is the bug the sign-off named.
- **CF1b** the `204` arm deleted → `204 No Content` case RED. **CF1c** the `304` arm deleted → `304` test RED.
  **CF1d** `304` ignores `get` → the unit test RED, and only that, as expected: no verb here is a non-GET.
  **CF1e** the HEAD check inverted → 4 tests RED. **CF1f** `content_length: None` loosened → the both-headers case RED.
- **CF2a** no `<IsTruncated>` reads as false → `2 case(s)`. **CF2b** any other value reads as false → `6 case(s)`.
  **CF2c** repeated field, last wins → `14 case(s)`. **CF2d** markup reads as its first text → `12 case(s)`. These are the
  adversary's four mutations, applied one at a time; together they survived iteration 2's whole crate suite.
- **CF3a** `object_length(framing).unwrap_or(None)` → RED: `Range 206 Partial Content no framing: streamed, declaring None`.
  **CF3b** `Framing::Undeclared => Ok(None)` → the same test RED.
- **CF4** the listing drops every ETag → exact-budget RED, `2 case(s)`.

The script was scratch (`$PDCA_SCRATCH/pdca-builder-955-reds/reds.py`) and is not kept.

## Refuting my own test

- **(a) Genuine red? Yes.** Fully reverted, the test does not build (iteration 2's run; same imports). Each single edit above
  turned named assertions red with the code compiling, and none stayed green.
- **(b) Production path? Yes.** Every client is built through the production `resolve_config` and `S3Client::with_deadlines`, so
  each call runs the real `aws-sdk-s3` orchestrator and the production `Capture`, `framing_of_no_body`, `read_whole`,
  `listing_page`, `object_length` and `ObjectBody`, over real loopback TCP. (A) and half of (H) hit a real
  `wyrd_gateway_s3::S3Gateway`. The scripted endpoint only writes fixed bytes. The three unit tests in `answer.rs` call the
  production functions directly.
- **(c) Fixture includes the fault? Yes.** The `304`s and HEAD answers really carry `gzip, chunked` on the wire, through real
  hyper. The cut cases really close the socket before the terminal chunk or the declared length. The spoiled pages are real
  documents with one change each, checked to contain the text being replaced.

## Mutation testing (C5)

Run through the gate's own script, `scripts/mutants-in-diff`, on this bundle's patch:

`113 mutants tested in 18m: 78 caught, 35 unviable`, exit 0. **Zero missed, zero timeouts.**

That run was on the patch just before one last edit, which corrected a version and line citation in a doc comment
(`answer.rs:494`, `hyper` 1.10.1 to 1.12.0). No code changed and the line count is the same, so I did not re-run it; the tests
were re-run after the edit.

Zero missed is weaker evidence than it looks. Iteration 2 also had zero missed while the adversary's hand-written mutations of
the refusal rules survived, because `cargo mutants` has no pattern for them. The hand-written reds above are the evidence for
those rules, this row is not.

## Commit-readiness

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-validate --all-targets` (workspace lints, warnings denied): clean. It first refused one test type as too
  complex; a local `type Headers` fixed that.
- `typos crates/validate`: clean.
- `cargo doc -p wyrd-validate --no-deps --document-private-items`, with `-D warnings` and `rustdoc::private_intra_doc_links`
  allowed: clean. I kept `framing_of_no_body` out of the public `BodyFailure` doc and described the rule in words there.
- I did not run the full `cargo xtask ci` (workspace build and test, DST included); Check's C4-ci runs it.

## External dependencies

`typos`, as the brief lists: present and run. Nothing else was needed: no Docker, no network beyond `127.0.0.1`. No NEEDS-HUMAN
external dependency.

## For the human at sign-off

1. The patch is about 183 KB, against the 100 KB size backstop you already overrode at 154 KB. Almost all of the growth is tests.
2. Still open from iteration 2: the `304`-with-`Content-Length` fix matches an SDK error message
   (`refused_by_the_sdks_length_check`, `crates/validate/src/s3/answer.rs:606`). It fails safe, but it is a string match.
3. Still open from iteration 2: the base `get_object` keeps the checksum flaw, and as far as I know has no issue of its own.
4. Still open from iteration 2: #956 has to add `SuccessBody` back with its probes (about 25 lines from the salvage).
5. The docs-currency finding depends on the batch reviewer honouring the new marker. The rubric says a deferral marker settles
   it, but I cannot prove the reviewer will behave that way until it runs.
