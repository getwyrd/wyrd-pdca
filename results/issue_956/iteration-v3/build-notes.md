# Build notes — #956 (743.2, second child of #938), iteration 3: bulk delete, probes, endpoint resolution

## Base and starting point

- Built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`, HEAD `635e0bd` (`pdca-integrate: issue_955`).
  `patch.diff` is the whole change against that commit: 6 files. The worktree is left at `635e0bd` with the change
  uncommitted.
- Salvage hashes rechecked, both as the brief states: `patch.diff` `b50cc4f5…7d69`, `build-notes.md` `f2c4fcf9…32f7`.
- Starting point: `iteration-v2/patch.diff`, which applied cleanly, because the sign-off says "Keep the routing, probe
  and row-reading code; it held up." Routing, probe dispatch and row reading are unchanged, except for the fixes below.
- `crates/validate/src/lib.rs`, `Cargo.toml`, `Cargo.lock`, `crates/validate/Cargo.toml` and the three older
  `s3_client_*` test files are untouched (`git diff --stat` on them is empty). `deny*.toml` is not in the diff.

## Iteration 2's carry-forward, item by item

Line numbers are in the patched tree. Tests are in `crates/validate/tests/s3_client_smoke_verbs.rs`.

| # | What the sign-off asked | What changed |
| --- | --- | --- |
| 1 | Refuse an overlong key or upload id before the SDK, as `RequestNotBuilt`, with a test | `Probe::too_long` (`crates/validate/src/s3/verbs.rs:131`), called first in `probe` (`verbs.rs:399`). Limits at `verbs.rs:33`, `:39`. Docs at `verbs.rs:61-67`, `crates/validate/src/s3.rs:52-56`, `docs/design/architecture/05-building-block-view.md:303`. Tests: `a_probe_whose_key_or_upload_id_is_too_long…` (`tests:1418`), `a_probe_whose_key_and_upload_id_just_fit…` (`tests:1469`). |
| 2 | Run `BODY_FAULTS` and the chunked 304 through `delete_objects` | `a_delete_result_answered_200_keeps_its_200…` (`tests:2270`), `a_304_that_frames_a_chunked_body_is_refused_for_a_bulk_delete…` (`tests:4741`). Tests only: production was already right. |
| 3 | Refuse a processing instruction inside `<DeleteResult>`; replace the test | `crates/validate/src/s3/document.rs:249-254`. New `SPOILED_ROWS` row (`tests:3190`). The old test is now `white_space_and_comments_beside_delete_result_rows_are_read_past` (`tests:3231`), with no processing instruction in it. |
| 4 | Keep 64 KiB; replace "many long keys"; state the limit with real numbers | `verbs.rs:348-354`, `05-building-block-view.md:303`. Pinned by `a_bulk_delete_whose_result_passes_the_budget…` (`tests:602`), since brief (H) forbids a sentence the tests do not show. |

### Choices inside item 1

- **Why the guard counts percent-encoded bytes.** The panic comes from the URI the SDK writes, not from the key's raw
  length. 30,000 `&` is 30,000 raw bytes and 90,000 written. A raw-length check let it through and the SDK panicked
  (red `it1b` below). The count follows the SDK's encoding: a key keeps its `/` as one byte, an upload id's `/` is three
  (`aws-smithy-http` 0.64.3 `src/label.rs:14`, `src/query.rs:17`, `src/urlencode.rs:9-42`). That set is the same one
  `path_segment` encodes (`s3.rs`), so I reuse `path_segment` and no new dependency is needed.
- **Why 64 bytes of room.** The panic comes at the SDK's `expect("valid request")` when its operation URI passes the
  `http` crate's 65,534 bytes (`aws-sdk-s3` 1.155.0 `src/operation/create_multipart_upload.rs:370`; `http` 1.4.2
  `src/uri/mod.rs:145`, `:296`). The most the SDK writes around a key and upload id is 40 bytes (`/` then
  `?x-id=UploadPart&partNumber=1&uploadId=`), so 64 leaves 24 spare for a later SDK.
- **The cost of that room, for your check.** A probe with 65,471 to about 65,500 encoded bytes could have been sent
  and is now refused. With the guard removed, `CopyObject` at 65,471 bytes did reach the gateway (501). I accepted this:
  S3's own key limit is 1,024 bytes, the report is still `RequestNotBuilt`, and the docs state the rule. An exact limit
  per probe needs six copies of the SDK's query strings to keep in step (about 8 lines), and it would turn into a panic
  if a later SDK wrote more.
- **A long `source_key` is not limited.** It goes in a header, not the URI. The test sends one of 70,000 bytes and reads
  it back from the request.

### Alternatives rejected for item 1

- **Catching the panic** (`AssertUnwindSafe(fut).catch_unwind()` in `probe`, about 6 lines). It treats the symptom, still
  prints a panic message to stderr, and would abort the process under `panic = "abort"`. The sign-off also asked for the
  refusal to come before the SDK is called.
- **Limiting keys to S3's 1,024 bytes.** Simpler (one comparison), but the client would then judge a key by S3's rules,
  against the brief's invariant that a request is not filtered on the way out.

### Choices inside the other items

- **Item 3: a processing instruction inside a row is still passed over.** The sign-off and the three cited sentences are
  about what the root holds. A row's `fields` reads child elements only, and it is #955's shared helper. Nothing in a row
  can hide another row, so I left it.
- **Item 4: the numbers are pinned exactly.** The test reads the gateway's `Content-Length` for two keys, one with `&`
  (`115 + 2×30 + key bytes`, with `&amp;` counted), then `62115` for 1,000 keys of 32 bytes, then
  `OverBudget { received: 70_115 }` for 1,000 keys of 40 bytes. A change in the gateway's markup fails this test, and the
  docs would then need the new numbers. That is what (H) asks for.
- **Item 4: what the test costs.** The 40-byte leg writes all 1,000 keys and checks all 1,000 are gone, so "after the
  gateway has deleted the keys" is shown for every key, not a sample. That test takes about 8 s, and the test file went
  from 1.7 s to 8 s. It also lengthens the C5 row: the mutation run was 18 minutes for 64 mutants, against 10 minutes for
  51 last time. The gate's limit is 7,200 s. Checking a 20-key sample would bring the test back under 1 s (a change of 4
  lines), but the claim would then rest on a sample. The 32-byte leg uses a sample of 20, because only the result's size
  is at stake there.

## The other failing gate from iteration 2

**C5 (advisory):** this run had **64 mutants: 47 caught, 14 unviable, 3 timeouts, 0 missed**, using the gate's own
arguments (`cargo mutants --in-diff patch.diff --no-shuffle --cap-lints=true`). All 14 new `Probe::too_long` mutants
were caught.

The 3 timeouts are the same three as before, now at `s3.rs:509` (`Ok("xyzzy")`) and `s3.rs:514` (the guard → `false`,
`||` → `&&`). I did not look into them again this round: the sign-off says "Fix these only", and iteration 2's notes and
the adversary's rerun both found them to be slow failures, inside `s3_client_nonconforming_responses`, which brief (E)
forbids changing. The row will keep showing `fail`, with nothing missed.

## Red → green

Verification posture (a), net-new. With production reverted, the test file does not build, as the brief pre-declares,
and the driver's C4-verify reports UNVERIFIABLE or green-only. I did not re-run that revert this round. The build
failure was shown in iteration 2 (`E0432` on the imports), and it cannot have changed.

Green: `cargo test -p wyrd-validate --test s3_client_smoke_verbs` → `52 passed; 0 failed`, 8 s.
**(E):** in `cargo test -p wyrd-validate`, `s3_client_nonconforming_responses` 28, `s3_client_roundtrip` 15,
`s3_client_upload_peers` 12, all passing, all three files unmodified.

### Demonstrated reds

Each is one edit to production, a run of the test file, then the file restored by a script. All ran this cycle, each
exited 101 with no build error, and the production code is the same as in the patch. After most of them ran I changed
test data only: the too-long case list and how many keys the budget test writes. I re-ran `it1a` and `it1b` after the
case-list change. The others were not re-run, and the final C5 run (0 missed) is on the final tree.

The six the brief requires:

| Red | Edit | Tests that fail, and how |
| --- | --- | --- |
| (1) | `probe` returns only the timeout when its body stalls after a 2xx | `a_probe_answered_200_keeps_its_200…`: `22 case(s) lost or misreported the 200` |
| (3) | The SDK's default endpoint resolution back (`DefaultResolver`) | `bucket_probes_send_every_name…`, `an_endpoint_path_comes_before…`, `the_region_is_signed_as_given…`, `a_request_stopped_before…` (`Invalid region`, not the query) |
| (5) | `<Error>` rows dropped | `a_mixed_delete_result…`, `a_delete_result_reports_requested_keys…`, `a_delete_error_row_with_no_code…` |
| (6) | A request refused before sending reported as `NoResponse` | `a_request_stopped_before…`, `a_read_verb_stopped_before…`, `a_probe_whose_key_and_upload_id_just_fit…` (`UploadPart: not the SDK's own refusal: Err(NoResponse…`) |
| (16) | A probe's expired deadline turned into `NoResponse` | both probe deadline tests |
| (17) | `UploadPart` with another `uploadId` | `every_unsupported_operation…`, `a_2xx_carrying…`, `a_304_that_frames_a_chunked_body_is_refused_only…` |

New for this iteration's items:

| Edit | Test that fails, and how |
| --- | --- |
| `it1a` Guard's result ignored | `a_probe_whose_key_or_upload_id_is_too_long…`: **the SDK panics**, `aws-sdk-s3-1.155.0/src/operation/copy_object.rs:399:62: valid request: http::Error(InvalidUri(TooLong))`. This is the reported defect, reproduced. |
| `it1b` Raw length, not encoded | Same test, the same panic, on the 30,000 `&` key |
| `it1c` A key's `/` counted as three | `…just_fit…` (`take 130940 bytes`) and `…too_long…` |
| `it1d` Upload id not counted | `…too_long…`: `UploadPart with an upload id that reaches the limit…` |
| `it1e` Limit one byte tighter (`>=`) | `…just_fit…`: `take 65470 bytes` |
| `it1f` Limit one byte looser | `…too_long…`: `one byte past the limit` |
| `it1g` No room left (`URI_ROOM = 0`) | `…too_long…` |
| `it2a` `NoAnswer` on a body timeout in `delete_objects` | `a_delete_result_answered_200…`: `2 case(s)` (stall, trickle) |
| `it2b` `NoAnswer` on a cut body in `delete_objects` | Same test: `1 case(s)` |
| `it2c` `Capture::new(true)` in `delete_objects` | `a_304_that_frames_a_chunked_body_is_refused_for_a_bulk_delete…` |
| `it3a` Processing instruction skipped | `a_delete_result_row_that_cannot_be_read…`: `1 case(s)` |
| `it3b` Comment refused | `white_space_and_comments_beside…` |
| `it4` Budget doubled | `a_bulk_delete_whose_result_passes_the_budget…`: 40-byte keys come back as a result, `Ok(DeleteResult…` |

`it2a`, `it2b` and `it2c` are the three mutations the adversary said survived in iteration 2. All now fail a test.

A correction made while doing this: my first list of too-long keys used 30,000 spaces, with a comment saying the SDK
would panic on them. `it1b` showed otherwise: the SDK reads an all-white-space key as missing and refuses it
(`RequestNotBuilt`, "key was missing"). I replaced the spaces with `&` and fixed the comment, and I moved the 70,000-byte
case first so that `it1a` reaches the panic and does not stop earlier on a request that could be sent.

Iteration 1's and 2's `cf*` edits were not re-run this round. That code did not change, and their tests still pass.

## Refuting my own test

- **(a) Genuine red? Yes.** A full revert does not build, the case the brief pre-declares. Beyond that, each behaviour was
  broken on its own and all 19 edits above failed a named test at assertion level. `it1a` reproduces the actual panic.
- **(b) Production path? Yes.** Every leg builds the client through `resolve_config` and `S3Client::with_deadlines`, so it
  runs the production `probe`, `too_long`, `delete_objects` and `delete_rows` in the real `aws-sdk-s3` orchestrator, over
  the client's own connector on real loopback TCP. The budget test runs against the real `S3Gateway`. No mock of the
  client, SDK or gateway.
- **(c) Fixture includes the fault? Yes.** The endpoint really stalls, trickles and cuts the delete result's body and
  really sends a chunked 304. The real gateway really returns 70,115 bytes after really deleting 1,000 written keys. The
  overlong inputs really go to `probe`, and the relay shows no connection was opened. What this does not include: real
  S3, whose result markup may differ from the gateway's. The docs give the numbers as the Wyrd gateway's.

## Commit-readiness

- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-validate --all-targets`: clean.
- `typos crates/validate docs/design/architecture/05-building-block-view.md`: clean. `lint_docs.py`: OK.
- Whole gate: `./engine/xtask.sh ci` on this tree: `xtask ci: all checks passed`.
- `patch.diff` compared byte for byte with `git diff --binary HEAD` after the last edit. The test file is also copied to
  the bundle as `s3_client_smoke_verbs.rs`, and compared the same way.
- Scratch: the mutation output and my edit scripts are in `$PDCA_SCRATCH`. I left them for the harness to reclaim, per the
  builder's filesystem rule.

## External dependencies

`typos` and `docs-renderer`, as the brief lists: both present and run. No Docker, no network beyond `127.0.0.1`.
No NEEDS-HUMAN external-dependency item.

## For your attention at sign-off

1. **`head_object`'s panic is still there, as you ruled.** It is #955's. The other verbs that name a key use the same SDK
   code path, so by reading they panic too. I ran none of them this round; the adversary ran `head_object`. The
   docs now say so at `s3.rs:52-56` and in the architecture paragraph ("the other verbs that name a key do not check its
   length"), rather than claiming every URL is covered. No issue tracks this yet, and I did not open one. It needs one.
2. **The guard refuses a narrow band of sendable probes** (about 30 bytes just under the URI limit). See "The cost of that
   room" above.
3. **The budget test adds about 7 s to the test file and about 8 minutes to the C5 row**, for all 1,000 keys being written
   and checked. A sample would remove most of that. See "what the test costs" above.
4. **C5 will still show `fail`** for the three known timeouts, with 0 missed.
