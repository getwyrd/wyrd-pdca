# Build notes — #956 (743.2, second child of #938), iteration 2: bulk delete, probes, endpoint resolution

## Base and starting point

- Built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`, HEAD `635e0bd` (`pdca-integrate: issue_955`), the
  bundle's `stack-base-tip` on `pdca-integration/main-rdfeb36496b2d`. `patch.diff` is the whole change against that
  commit: 6 files. The worktree is left at `635e0bd` with the change uncommitted.
- Salvage hashes rechecked, both as the brief states: `patch.diff` `b50cc4f5…7d69`, `build-notes.md` `f2c4fcf9…32f7`.
- Starting point: `iteration-v1/patch.diff`, which applied cleanly, because the sign-off says "Keep the routing and
  probe code; it held up." Everything iteration 1's notes say about fitting the salvage to #955's tree still stands
  (`iteration-v1/build-notes.md`, "Where the salvage did not fit"). Routing and probe dispatch are unchanged here.
- `crates/validate/src/lib.rs`, `Cargo.toml`, `Cargo.lock`, `deny*.toml` and the three older `s3_client_*` test files
  are untouched (`git diff --stat` on them is empty).

## Iteration 1's carry-forward, item by item

Line numbers are in the patched tree.

| # | What the sign-off asked | What changed |
| --- | --- | --- |
| 1 | Do not skip unknown children of `<DeleteResult>`; replace the `<RequestCharged>` test | Refused: `crates/validate/src/s3/document.rs:266-270`. Module doc: `document.rs:9-11`. Rows in `SPOILED_ROWS` (`tests/s3_client_smoke_verbs.rs:2806`). The old test is now `a_delete_error_row_with_no_code_or_message_reports_none_and_a_rows_own_other_fields_are_passed_over` (`tests:2929`). It uses `<VersionId>`, `<DeleteMarker>` and `<DeleteMarkerVersionId>`, which do occur inside a row. |
| 2 | Report requested keys with no row and rows naming no requested key, without judging; test empty, partial, unrequested | `DeleteResult::requested` (`document.rs:84`), `keys_without_a_row` (`:107`), `rows_not_requested` (`:116`), `DeleteRow::key` (`:135`), set at `verbs.rs:307`. Test: `a_delete_result_reports_requested_keys_with_no_row_and_rows_naming_no_requested_key` (`tests:2698`). `verbs.rs:290-298` and the architecture doc reworded. |
| 3 | Empty `<Key/>` is no key, matching `named_code` | `named_key` (`document.rs:277`), used at both rows. Three `SPOILED_ROWS` rows. See the difference from `named_code` below. |
| 4 | Record bodies in the relay; assert `UploadPart`, `CompleteMultipartUpload`, `CreateBucket` | `Request` and `unframed` (`tests:1581`, `:1596`), the relay records them (`tests:654`, `:673`), `SentBody` (`tests:712`), checked through `sent_fault` (`tests:807`) in `probe_reaches_the_gateway`. |
| 5 | Safety depends on the caller's name; carry the dot-segment exception over; add `.` and `..` rows | `verbs.rs:36-40` and `docs/design/architecture/05-building-block-view.md:303`. Rows at `tests:176-177`, against the real gateway on both endpoint forms. |

### Choices inside those items

- **Refuse, not report (item 1).** The sign-off allowed either. Reporting needs a new typed field and `<error>`, the
  adversary's example, would then have to be read as a row or as an unknown. A refusal fits the document's other rules:
  a field given twice is refused too. Cost if S3 ever adds a child: a call is refused until this is revisited, loudly.
  The SDK's deserializer reads only `Error` and `Deleted` here (`aws-sdk-s3` 1.155.0
  `src/protocol_serde/shape_delete_objects.rs:143`, `:160`), and reads `RequestCharged` from a header (`:30-31`).
- **Stray text is refused as well (`document.rs:243-246`).** The sign-off said "unknown children". Non-white-space text
  is one, and the same skip would have lost it. Five lines. White space, comments and processing instructions are read
  past, with their own test (`tests:2912`), so a pretty-printed document is not refused.
- **Where this differs from the sign-off's wording: `named_code` also treats all-white-space as empty; `named_key` does
  not.** An S3 key of one space is legal, so refusing it would refuse a true row. The `as-sent` case pins this and red
  `cf3b` shows it binds. Flagging it for your check at sign-off.
- **Reported, not refused (item 2).** A `200` with `<DeleteResult/>` stays `Ok`, as the sign-off requires. Judging stays
  with #937. `requested` is kept on the result so the caller has what it needs. Comparison is exact text after XML
  escapes are read. A key asked for twice and missing is listed twice; a key answered twice shows both rows.
- **Methods, not stored lists (item 2).** The two reports are computed from `requested` and `rows`, so they cannot
  disagree with them. Each builds one `HashSet` per call; rows are capped by the 64 KiB budget.
- **Every probe's body is checked, not only the three named (item 4).** All twelve go through `SentBody`, at no extra
  code: nine others are `Empty`, and that is what they send. `unframed` also undoes chunked and `aws-chunked` framing,
  since the brief says framing is not asserted. In this run no probe body uses `aws-chunked`, so that branch runs only
  if the SDK starts framing them.
- **Forwarding changed (item 4).** The relay now reads the whole request before forwarding, reusing `read_request`
  (`Expect: 100-continue` would stall this; the tests show no request sends it).
- **`.` and `..` against the gateway.** Each comes back as the gateway's own `400 InvalidRequest`, so those rows pass
  the same checks as every other name.
- **Also trimmed:** a test comment claimed `wyrd-validate-probe-` was "the one 743.3 probes with". Which name is 743.3's
  to choose (brief, prior-art section), so the claim is gone.

## The other failing gates from iteration 1

- **T4:** the finding at `verbs.rs:325` is addressed by item 2. **I did not write `review-rejected.md`.** The sign-off
  asks for that only if T4 still raises it, and recording a rejection up front could hide a real new finding. If it
  does come back, a line to paste (check `MATCH` against the new wording):
  `crates/validate/src/s3/verbs.rs:<line> | BUG | without accounting for | The result reports requested keys with no row and rows naming no requested key (DeleteResult::keys_without_a_row, rows_not_requested). Judging them is #937's, out of this child's scope (brief.md scope line).`
- **C4 diff coverage ("does not apply on origin/main"):** not fixable from this patch. It stacks on #955 by design
  (`stack-base`), and the gate's script compares with `origin/main`. It will fail the same way until #955 lands on
  `main`. Coverage stays unmeasured; the row says nothing either way.
- **C4-verify:** as the brief pre-declares, red with production reverted is a build failure (below).
- **C5 (3 timeouts):** same three as before, none missed. See "Mutation testing": this time I read the logs.

## Red → green

Verification posture (a), net-new. With production reverted and the test file kept, the file does not build: exit 101,
`error[E0432]: unresolved imports wyrd_validate::s3::DeleteRow, KeyError, Probe`. The driver's C4-verify is expected to
report this as UNVERIFIABLE (77) or green-only, as the brief says.

Green: `cargo test -p wyrd-validate --test s3_client_smoke_verbs` → `47 passed; 0 failed`, 1.7 s.
**(E):** `cargo test -p wyrd-validate`: `s3_client_nonconforming_responses` 28, `s3_client_roundtrip` 15,
`s3_client_upload_peers` 12, all passing, all three files unmodified.

### Demonstrated reds

Each is one edit to production, a run of the test file, then the file restored. The tree was compared byte for byte
with `git diff` after each batch. Each exited 101 with no build error. All were run this cycle against this tree.

The six the brief requires:

- **(1) Only the timeout when a probe's body stalls after a 2xx** → `a_probe_answered_200_keeps_its_200…`: `22 case(s)
  lost or misreported the 200: CopyObject Stall: the 200 head was lost: Timeout { phase: BodyIdle, limit: 500ms }`.
- **(3) The SDK's default endpoint resolution back** → 4 tests: `bucket_probes_send_every_name…` (`42 case(s) not sent
  as one path segment`, e.g. `CreateBucket "wyrd-validate--x-s3" … not signed for s3 in us-east-1`);
  `an_endpoint_path_comes_before…` (`("PUT", "/s3-prefixwyrd-validate-probe-/")`); `the_region_is_signed_as_given…`
  (`aws-global: signed for /aws-global/s3/aws4_request`); `a_request_stopped_before…` (`Invalid region`, not the query).
- **(5) Drop the `<Error>` rows** → `a_mixed_delete_result…`: `left: [Deleted { key: "mixed/a" }, Deleted { key:
  "mixed/c" }]`; two other delete tests also fail.
- **(6) A request refused before sending reported as `NoResponse`** → `a_request_stopped_before…` (`CreateBucket of a
  70,000-byte name: not refused as unsent: Err(NoResponse…`) and #955's `a_read_verb_stopped_before…`.
- **(16) A probe's expired deadline turned into `NoResponse`** → both deadline tests: `left: Some(NoResponse { detail:
  "" }) right: Some(Timeout { phase: Operation, limit: 500ms })`, and likewise `Connect`.
- **(17) `UploadPart` with another `uploadId`** → (B), (G) and the 304 test: `its operation parameters are
  [("partNumber", "1"), ("uploadId", "wyrd-validate-probe-upload-other")], not …`.

New for this iteration's items:

| Edit | Test that fails, and how |
| --- | --- |
| `cf1a` Unknown child skipped again (`_ => {}`) | `a_delete_result_row_that_cannot_be_read…`: `3 case(s)`, including `error-row-in-the-wrong-case … Ok(DeleteResult…` |
| `cf1b` Stray text skipped | same test: `text-outside-any-row: not refused` |
| `cf1c` White space refused too | `white_space_comments_and_processing_instructions…`: `holds text outside any row` |
| `cf2a` `keys_without_a_row` reports nothing | `a_delete_result_reports…`, case `empty` |
| `cf2b` `keys_without_a_row` inverted | that test, the mixed test and the gateway test |
| `cf2c` `rows_not_requested` reports nothing | `a_delete_result_reports…`, case `unrequested-deleted` |
| `cf2d` `rows_not_requested` inverted | that test, the mixed test and the gateway test |
| `cf2e` `requested` left empty | same three tests |
| `cf3a` Empty `<Key/>` counts as a key | `a_delete_result_row_that_cannot_be_read…`: 3 cases |
| `cf3b` White-space key refused | `a_delete_result_reports…`, case `as-sent` |
| `cf4a` `UploadPart` sends `hello` | `every_unsupported_operation…`: `its body is "hello", not empty` |
| `cf4b` `CompleteMultipartUpload` names no part (first try was a dead-code build error; redone so it compiles) | same test: `its body is "<CompleteMultipartUpload …></CompleteMultipart…` |
| `cf4c` `CompleteMultipartUpload` names part 2 | same test |
| `cf4d` `CreateBucket` sends `<CreateBucketConfiguration>` | same test: `its body is "<CreateBucketConfiguration…` |
| `cf5` Dots percent-encoded in every name | `bucket_probes_send_every_name…` (16 cases) and `an_endpoint_path_comes_before…` |
| `cf5b` Only `.` and `..` percent-encoded | same two tests: `8 case(s)`, `CreateBucket "." … reached the gateway as PUT /%2E/`. This one binds the new rows. |

These three were the adversary's surviving mutations in iteration 1 (`cf4a`, `cf4b`, `cf4d`); all now fail a test.

## Mutation testing (C5, advisory)

`cargo mutants --in-diff patch.diff --no-shuffle --cap-lints=true`, the gate's own arguments, output in scratch:
**51 mutants: 34 caught, 14 unviable, 3 timeouts, 0 missed**, 11 minutes (exit 3 means "some timed out").

The 3 timeouts are the same as before: `s3.rs:507` → `Ok("xyzzy")`, the `s3.rs:512` guard → `false`, and its `||` →
`&&`. I read their logs this time. In all three, the run is inside `s3_client_nonconforming_responses`: 10 tests have
already failed and 23 report "has been running for over 60 seconds". Those tests wait on a scripted endpoint for a
request that these mutants stop from ever being sent. So each mutant is detected, and the time goes on that older
file's waits. Brief item (E) forbids changing that file, so I left it. The gate runs that file before this one, and so
does not get as far as the smoke tests under these three mutants.

## Refuting my own test

- **(a) Genuine red? Yes.** Fully reverted, the file does not build, the case the brief pre-declares. Beyond that, each
  behaviour was broken on its own and all 22 edits above failed a named test, assertion-level (`cf4b`'s first attempt
  aside, redone).
- **(b) Production path? Yes.** Every leg builds the client through `resolve_config` and `S3Client::with_deadlines`, so
  it runs the production `delete_objects`, `delete_rows`, `probe` and `PathStyle` in the real `aws-sdk-s3` orchestrator,
  over the client's own connector on real loopback TCP. (A)/(B) hit the real `S3Gateway`. The relay only records and
  forwards bytes. No mock of the client, SDK or resolver.
- **(c) Fixture includes the fault? Yes.** The delete fixtures send the faulty bytes themselves: `<DeleteResult/>`, a
  partial answer, an unrequested key, `<error>`, `<Key/>`. The body checks read what the SDK actually put on the wire
  through the relay, in front of the real gateway. `.` and `..` go to the real gateway on both endpoint forms. What I
  could not include: a real server that resolves dot segments. The gateway does not, so the doc's sentence about such a
  server describes a risk and no test shows one acting on it. What the rows do show is the wire form.

## Left alone on purpose

- `<Message/>` stays `Some("")`: as sent. The sign-off named `<Key>` only, and the type already says a message is
  whatever was given.
- `.force_path_style(true)` stays beside `PathStyle`, unread by it (same as iteration 1).
- `not_built` (`verbs.rs`) has no test: the SDK's builders only refuse a missing `key` or `objects`, and
  `delete_objects` sets both. A later SDK that refuses more gets `RequestNotBuilt` rather than a panic.
- `delete_objects` of no keys is sent as asked. Nothing in the brief asks to refuse it, and the server's answer is what
  the validator is there to report.
- The `<ListBucketResult>` rules (#955's, unknown elements passed over) are unchanged: out of scope.

## Commit-readiness

- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-validate --all-targets`: clean.
- `typos crates/validate docs/design/architecture/05-building-block-view.md`: clean. `lint_docs.py`: OK.
- Whole gate: `./engine/xtask.sh ci` on this tree: `xtask ci: all checks passed`, exit 0. It covers docs render, DST and
  the statics and blackbox guards.
- The repo checkout has no pre-commit config or installed hook; `cargo xtask ci` is its gate.
- `patch.diff` compared byte for byte with `git diff --binary` at the end; it applies in reverse on this tree.
- The test file is also copied to the bundle as `s3_client_smoke_verbs.rs`.

## External dependencies

`typos` and `docs-renderer`, as the brief lists: both present and run. No Docker, no network beyond `127.0.0.1`.
No NEEDS-HUMAN external-dependency item.

## For your attention at sign-off

1. The `named_key` difference from `named_code` above: white-space keys are read as keys, on purpose.
2. Unknown `<DeleteResult>` children are refused rather than reported. You offered either.
3. If T4 raises the empty-result finding again, it needs the rejection line above, and that is yours to record.
