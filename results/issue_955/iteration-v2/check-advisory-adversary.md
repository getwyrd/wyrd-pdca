# Adversarial review: issue 955 (S3 client reads and listings)

The core design held: I could not make the client lose a 2xx head, accept a cut page, or report the wrong error code. I did find one wrong report on a real input and two groups of refusal rules that no test protects. I tested in a scratch copy of the target, with toolchain 1.96.0 available, and have removed that copy.

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/answer.rs:493-502` with `crates/validate/src/s3/response.rs:106`: a bodiless answer that carries a multi-coding `Transfer-Encoding` is reported as a failed body. **Failing case 1:** `304 Not Modified` with `Transfer-Encoding: gzip, chunked`, in answer to `get_object_if_none_match`, gives `Outcome::BodyFailed(Framing { transfer_encoding: Some("gzip, chunked"), .. })`. A plain `chunked` 304 gives `Error { NoBody }`. **Failing case 2:** `head_object` answered `200` with the same header gives the same `BodyFailed`. I ran both. `response.rs:115-117` lets this 304 past its own refusal check, but `:106` has already marked it `Untrusted`, and `read_whole` returns on that before it reads anything. Brief (G) says a GET's or HEAD's 304 "may carry framing headers with no body", and RFC 9112 §6.1 allows the header on a 304 to a GET. So a conforming answer is downgraded, against the brief's invariant. The test at `crates/validate/tests/s3_client_smoke_verbs.rs:1456` covers only exactly `chunked`. Fix it in the new read path, not in `Framing::of`, because the older verbs share `Framing::of` and must not change. The HEAD `200` case is the weaker of the two: if the builder treats it as intended, it should say so in a comment and pin it in a test.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/document.rs:102-111` and `:157-161`: the rules that keep a listing from reading as complete when it isn't have no test. Production is correct today: I sent a page with no `<IsTruncated>`, one with `TRUE`, one with the field twice, and one with a comment inside it, and all four were refused. But I applied these mutations together and every test in `cargo test -p wyrd-validate` still passed:
  - a missing `<IsTruncated>` reads as `false`
  - any other value reads as `false`
  - a repeated field is accepted, last one wins
  - markup in a field is read as its first text

  With those mutations, a page with no `<IsTruncated>`, or with `<IsTruncated>TRUE</IsTruncated>`, ends the listing as a success with its entries so far, which is the partial listing brief (D) forbids. This is the rubric's "absent entries" class. Add scripted cases for all four through both listing verbs.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/verbs.rs:271` with `crates/validate/src/s3.rs:424-431`: no test checks the framing refusal on a ranged or conditional GET's 2xx. Production is correct today: a `206` with no framing gives `body: Err(LengthUndeclared)`, and one with `Transfer-Encoding: gzip` gives `body: Err(Framing)`. But changing `:271` to `object_length(framing).unwrap_or(None)` left the whole crate's tests passing, as part of the same combined mutation run. That change would accept a close-delimited `206`, so a cut connection could read as the end of the object. The older `get_object` tests can't catch this because they don't go through `fetched`, and the framing test skips this case on purpose (`s3_client_smoke_verbs.rs:1627`). Add `Fetched::Object` cases for no framing and for an untrusted coding, plus a chunked 2xx and a body cut after a `206`; `s3.rs:421` shows chunked also never runs through the new test file.

- NEEDS-HUMAN [human] — `check-gates.json`, row `T4-batch-review`: the overall result is `fail`, and two of its three blockers are docs-currency findings at `crates/validate/src/s3/verbs.rs:42`. The brief assigns the architecture doc to #956, but the patch carries no deferral marker for it, and the rubric calls docs currency "a merge requirement, not a follow-up". `docs/design/architecture/05-building-block-view.md:299` describes `validate`'s command line, not its client methods, so these methods may not count as an "API operation" at all. A human should either reject the finding with that reason or record the deferral to #956. Otherwise the gate stays red.

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:1118`: I agree with the third T4 blocker. The exact-budget case asserts `entries.len() == 2`, which is the count-only check the rubric rejects. Assert the keys and sizes instead, using the existing `keys_and_sizes` approach.

## Gate claims that overstate the evidence

These need no separate action; the findings above cover the fixes.

- **`C5-mutants` "107 tested: 73 caught, 34 unviable":** this is true but gives less comfort than it looks. My hand-applied mutations of the refusal arms all survived, so zero missed does not mean those refusals are tested.
- **`C4-diff-cov` 92.1%:** this clears the floor, but its MISS list includes the untested refusal lines above: `document.rs:105-110`, `document.rs:158-161` and `s3.rs:424-431`.

## Attempted to refute, could not

- **The required reds are real.** `C4-verify` is UNVERIFIABLE, as the brief declared in advance, because the test can't compile against the base. So I applied each failure to the fixed code by hand and ran the test file. These are mutations of the fixed code, not a pre-fix run.

  | Change | Tests that went red |
  |---|---|
  | Red 1: a timeout replaces the 2xx | the (C) listing test |
  | Red 2: a cut page is accepted | 3 tests, including the (D) test |
  | Red 4: listing stops after page one, V2 and V1 | 3 tests each, including the (A) test |
  | Red 15: every error reads as `NoBody` | the (H) test and the checksum test |
  | (G)'s 304 framing read as a non-GET | the chunked 304 test |
  | Checksum override removed, range and conditional GET separately | both checksum tests each time |

- **Production path.** The tests call the public `S3Client` verbs against a real gateway or a scripted socket, and they check the requests that were sent. I found no parallel copy of the production logic.
- **Empty ETag.** `get_object_if_none_match(key, "")` still sends an `if-none-match:` header, so it does not silently become an unconditional GET.
- **SDK length check.** By reading `content_length_enforcement.rs`: it only wraps GETs, ignores a `Content-Length` it can't parse, and only runs its comparison after the inner body ends. So a `5, 5` length should not cause a false refusal, and bytes after a 304's head stay an overrun, which the existing test confirms.
- **Requests the caller did not ask for.** The listings add no `encoding-type`, and HEAD has no checksum validator, by reading SDK 1.155.0. The checksum override does stop the validator on both GET verbs, as the mutation runs above show.
- **Scope.** The patch does not touch `lib.rs` or the three existing test files, and `wyrd-traits` is only a dev-dependency.
- **S3 Express before the connector.** `verbs.rs:204` carries `// deferred: #956`, so I treated it as settled and did not attack it.

<!-- pdca:leaf-complete -->
