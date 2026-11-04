# Build notes — #956 (743.2, second child of #938), iteration 6: bulk delete, probes, endpoint resolution

## Base and starting point

- Built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`, HEAD `635e0bd` (`pdca-integrate: issue_955`).
  `patch.diff` is the whole change against that commit: 7 files (v5's six plus `crates/validate/src/s3/response.rs`,
  docs only), sha256 `1e475446…5cef1`. The worktree is left at `635e0bd` with the change uncommitted.
- Salvage hashes rechecked, both as the brief states: `patch.diff` `b50cc4f5…7d69`, `build-notes.md` `f2c4fcf9…32f7`.
- Starting point: `iteration-v5/patch.diff`, which applied cleanly (`git apply --check` exit 0), because the sign-off
  says "Keep the routing, probe and row-reading code; it held up… Fix these only… Nothing else changes."
- **No production logic changed this round.** The production-file changes are doc comments: two paragraphs in
  `response.rs` (item 1) and one in `document.rs` (item 2). Everything else is in the test file.
- `crates/validate/src/lib.rs`, `Cargo.toml`, `Cargo.lock`, `crates/validate/Cargo.toml`, the three older
  `s3_client_*` test files, `deny*.toml` and `crates/gateway-s3` are untouched. The architecture doc is unchanged from
  v5 (none of the four items names it for a change; item 3 asks for a test of what it already says).
- The whole change since v5, checked file by file: `verbs.rs`, `answer.rs`, `s3.rs` and the architecture doc have
  diffs byte-identical to v5's patch sections (compared with `index` lines stripped); `document.rs` differs from
  v5's only by the one reworded paragraph; `response.rs` is new to the patch, two paragraphs. The test file, diffed
  against v5's copy (the bundle's `s3_client_smoke_verbs.rs` of 12:24 was v5's, byte for byte): the header (three
  sentences), one new gateway test plus its `CR_KEYS` table, one new `Case` in an existing test, the `step_by(50)`
  sampling in one test, and one new scripted test plus its `Sizing` enum. Nothing else.

## Iteration 5's carry-forward, item by item

Line numbers are in the patched tree. `tests` is `crates/validate/tests/s3_client_smoke_verbs.rs`.

| # | What the sign-off asked | What changed |
| --- | --- | --- |
| 1 | `response.rs:13-14` and `:45-46` say a PUT or DELETE success has no body, false for `Probe::CopyObject`; limit both to the PUT and DELETE of one object; add probes and delete results to the list at `:44`. Docs only. | `response.rs:11-16` (module doc) and `:44-51` (`BUFFERED_BODY_BUDGET` doc). Both sentences now say "the success of the PUT or DELETE of one object", both name `CopyObject` as the PUT whose success carries a document, and the `Answer` list (`:46-47`) reads "a HEAD's, a probe's, a listing page's, a delete result's, and a ranged or conditional GET's that is not a 2xx". No code. |
| 2 | `document.rs:75-76` says a key is compared "exactly as sent once the document's XML escapes are read", false for a key holding a carriage return; name line-ending normalisation and pin the case with a test; do not change the gateway. | `document.rs:75-83`: the sentence now names XML 1.0 §2.11 as `roxmltree` applies it (a raw CR, alone or before LF, read as one LF; `&#13;` read as the CR), says the Wyrd gateway writes the key raw so the row comes back changed, and that case and white space are compared as they are. Pinned twice: against the real gateway (`tests:605-660`, `a_key_holding_a_carriage_return_is_deleted_but_its_row_comes_back_with_a_line_feed`, with its `CR_KEYS` table) and against the scripted endpoint (the `line-endings` case, `tests:3258-3276`). The gateway is untouched. |
| 3 | `s3.rs:31-32` and the architecture doc say the budget caps a probe's body, but no test shows it: turning a probe's `BodyFailed(OverBudget)` into `Err(NoResponse)` at `verbs.rs:475` passes all 53 tests. Add an over-budget leg and an exactly-the-budget leg for a document probe. | `tests:4878-4984`, `a_probe_body_past_the_budget_is_refused_beside_its_200_and_one_of_exactly_the_budget_is_read`: for every probe whose success is a document (the seven `success_script` gives a body), three legs: one byte past the budget with a `Content-Length`, one byte past it chunked, exactly the budget. The sign-off's mutation is red A below. |
| 4 | `tests:657-671` does 1,000 PUTs and 1,000 HEADs on fresh connections; sample them as the earlier block does with `step_by(50)`. | `tests:718-733`: both loops `over.iter().step_by(50)`, 20 PUTs and 20 HEADs, with a comment saying why (the size comes from the gateway's `Content-Length`; a key with no object has its row too; 2,000 fresh connections leave 2,000 sockets in `TIME_WAIT`). The `70115` and `OverBudget` assertions are unchanged. The test binary went from v5's 6.9 s to 1.7 s. |

### Item 1: what each sentence now matches

- "the success of the PUT or DELETE of one object at nothing" (`response.rs:13-14`): the "capped at nothing" rule is
  `ResponseGuard::buffered`'s, and that guard is attached only on `put_object`, `get_object` and `delete_object`
  (`crates/validate/src/s3.rs:284`, `:327`, `:353`). A `CopyObject` is sent by `S3Client::probe` with a `Capture`
  (`verbs.rs:408`) and no guard; its `200` carries `<CopyObjectResult>` (`Probe::success_body`, `verbs.rs:165`) and
  is read as a success (`a_2xx_carrying_the_operations_whole_s3_success_is_a_success`). The parenthesis in the module
  doc (`:14-16`) points at the paragraph below it (`:23-25`), which already says the `super::answer` verbs take the
  body at the same hook and read it themselves.
- "on the PUT, GET and DELETE of one object: every error response, and every PUT or DELETE response"
  (`response.rs:44-45`): the same split v5 made in `s3.rs:29-31` for the same sentence. The sign-off named `:44` and
  `:45-46` (now `:46-50`); the first half of that same sentence carried the same over-claim, so I corrected it with
  them rather than leave the twin for the next review (v5 did the same at `s3.rs:25-27`, and the sign-off did not
  object). If you want the named lines only, reverting `:44-45` to "every body the SDK reads whole (every error
  response, and every PUT or DELETE response)" is one line.
- "a probe's, … a delete result's" in the `Answer` list (`response.rs:46-47`): `Capture::new` → `read_whole` under the
  budget (`answer.rs:558`, `:618-619`) on both (`verbs.rs:375`, `:408`), now pinned by item 3's test for a probe and
  by `listing_and_delete_result_documents_that_are_not_whole…` for a delete result.
- "a probe's success may carry a document (a `CopyObject`'s `<CopyObjectResult>`), read under the budget as any
  answer's body is" (`response.rs:49-51`): item 3's `CopyObject Exact` leg is exactly that, a 65,536-byte
  `<CopyObjectResult>` read as a success, and its two over-budget legs are the budget on it.

### Item 2: the rule, where it comes from, and what the two tests show

The reader is `roxmltree` 0.21.1 (`Cargo.lock`). Its `process_text` (`src/parse.rs:1036-1048`) hands any text that
holds `&` or `\r` to `TextBuffer::push_from_text` (`:1373-1390`), whose comment is "Translate \r\n and any \r that is
not followed by \n into a single \n character" with the §2.11 link. A character reference (`&#13;`) goes through
`push_raw` instead (`:1052-1054`, `is_as_is`), so it is kept as the CR. That is the XML rule: line endings are
normalised before the document is parsed, and references are expanded after.

The gateway writes a row's key through `xml_escape` (`crates/gateway-s3/src/lib.rs:3803-3812`), which escapes the five
predefined entities and nothing else, so a key holding a CR is written raw and comes back as an LF. The sign-off's
observation, reproduced by the gateway test (green run below): `delete_objects(&["cr\rkey", "crlf\r\nkey"])` is
`200`, both objects are gone (HEAD `404`), both rows are `<Deleted>`, `deleted()` is `["cr\nkey", "crlf\nkey"]`,
`keys_without_a_row()` is both requested keys, and `rows_not_requested()` is both rows. The objects really are
deleted because the SDK writes the request body's `<Key>` with the CR as a character reference, so the gateway
deletes the key as asked; only the answer loses it.

The scripted `line-endings` case pins the client's own reading apart from the gateway: a raw `\r` and a raw `\r\n`
in a `<Key>` read as `\n` (rows not requested; requested keys without a row), and `escaped&#13;key` reads as
`escaped\rkey` and names its requested key. Together they show exactly what the reworded sentence says and no more.

The test's doc (`tests:605-610`) says that fixing the gateway to escape the CR would make each row name its key, and
that this test would then say so. That is on purpose: the sentence in `document.rs` names the gateway's behaviour,
and this test is what tells you when it stops being true.

### Item 3: the legs, why every document probe, and the pin on the set

The sign-off asks for "a document probe". The test runs the three legs for each of the seven probes whose success is
a document (`CopyObject`, `CreateMultipartUpload`, `CompleteMultipartUpload`, `ListParts`, `ListMultipartUploads`,
`GetBucketLocation`, `ListBuckets`), 21 legs, because they cost nothing to add: each is one scripted connection and
64 KiB on loopback, and `success_script` already gives the document. The five headers-only probes are skipped in the
loop with a comment: a body of any size is not their success, and `a_probe_answered_200_keeps_its_200…` has their body
faults. Because that skip is a `continue`, the test ends by asserting the list of probes it sized is exactly those
seven (`tests:4971-4983`), so a success script changing to return no body cannot thin the legs out unnoticed (the
rubric's "a count-based assertion that can pass while the property fails", applied to the test's own coverage). That
assertion was added after the first green run, and the final green run below is on the tree with it.

Each leg checks the request first (`ProbeCase::request_fault`, as the (G) success legs do, brief (G) "a success
answered to the wrong request cannot pass"), then that the `200` and its request id are kept, then the outcome:
`BodyFailed(OverBudget { received })` with `received > BUFFERED_BODY_BUDGET` for the two over-budget legs (the bytes
held when the budget is crossed depend on how the response split over TCP, so only "past the budget" is asserted,
as iteration 3's sign-off asked for the delete result), and `Success { body }` with the body as sent for the
exactly-the-budget leg. The document is padded with a comment after the root (`padded`, the (D) helper), which
`whole_document` reads past for a probe's success as it does for a listing page (`SuccessBody::Document` →
`whole_document`, `answer.rs:351`).

### Item 4: what the sampling keeps

The block's two claims are unchanged: the gateway declares `70115` for 1,000 keys of 40 bytes, and the result is
refused past the budget beside its `200`. Both come from the response, not from the objects. The 20 HEADs still
show the gateway deleted what was there. 1,000 keys of 32 bytes (`tests:707-716`) were already sampled this way.

## Red → green

Verification posture (a), net-new. With production reverted, the test file does not build, as the brief pre-declares.
I did not re-run that revert. The build failure was shown in iteration 2 (`E0432` on the imports) and cannot have
changed.

All runs below are through the project's runner, `./engine/xtask.sh ci` (the configured `C4-ci` gate, which cd's into
`$PDCA_WORKTREE` and runs `cargo xtask ci`: fmt, clippy, build, the whole test suite, deny, conformance, typos, docs).
`cargo xtask` has no narrower test subcommand (`xtask/src/main.rs:119-160`), so each run is the whole gate, about
3 min 40 s. Logs are in `$PDCA_SCRATCH/pdca-builder-956-it6-*.log`.

### Red (`it6-red-ci.log`): two production mutations at once, each failing its own test

Both mutations in one run, since they touch different code and fail different tests:

- **Red A (item 3, the sign-off's mutation).** `verbs.rs:475`: after `self.answer(...)`, an outcome of
  `BodyFailed(OverBudget { .. })` returned as `Err(S3Error::NoResponse { detail: "MUTANT: …" })`.
- **Red B (item 2).** `DeleteResult::keys_without_a_row` (`document.rs:107-113`): the requested key's line endings
  normalised (`\r\n` and `\r` to `\n`) before the lookup, so a row the server rewrote "accounts for" the key: the
  client filling in on the server's behalf, which the invariant forbids.

Result: clippy and the build passed (so the new test code compiles clean under `-D warnings`); `s3_client_smoke_verbs`
`FAILED. 52 passed; 3 failed`, every other test binary green, `EXIT=1`. The three failures, each the one its mutation
predicts (line numbers as the file stood in that run; the pin added afterwards moved the budget test's assert from
4961 to 4962 and nothing else):

```
a_probe_body_past_the_budget_is_refused_beside_its_200_and_one_of_exactly_the_budget_is_read  tests:4961
  14 leg(s) not read against the budget as they must be:
  CopyObject OverWithLength: the 200 head was lost: Err(NoResponse { detail: "MUTANT: a probe body past the budget" })
  CopyObject OverChunked: … (and the same for the other six document probes, both over-budget legs)
  — while all 7 `Exact` legs printed GREEN: a success, with a 65536-byte body.      [red A]

a_key_holding_a_carriage_return_is_deleted_but_its_row_comes_back_with_a_line_feed              tests:637
  the keys as requested have no row:  left: []   right: ["cr\rkey", "crlf\r\nkey"]            [red B]
  (the earlier assertions passed: 200, two <Deleted> rows reading "cr\nkey" and "crlf\nkey")

a_delete_result_reports_requested_keys_with_no_row_and_rows_naming_no_requested_key            tests:3291
  line-endings: the requested keys no row names:  left: ["escaped\rkey"]   right: ["cr\rkey", "crlf\r\nkey"]   [red B]
```

Red B's scripted failure shows the mutation inverting the report: the normalised `cr\rkey` "found" its rewritten row,
and `escaped\rkey`, whose row was correct, stopped matching. That is the client changing what the server said.

### Green (`it6-green-ci.log`, then `it6-green2-ci.log`)

Mutations reverted (`verbs.rs` and `document.rs` back to v5's hunks, checked by diff). First run: `xtask ci: all
checks passed`, `EXIT=0`, `s3_client_smoke_verbs` 55 passed. I then added the `sized` assertion (item 3, above), ran
`cargo fmt --all` and `typos`, and ran the gate again on the final tree, `it6-green2-ci.log`:

```
xtask ci: all checks passed                                 EXIT=0   (15:22:47 → 15:26:28)
s3_client_smoke_verbs            55 passed; 0 failed   1.70 s
s3_client_nonconforming_responses 28 passed            0.41 s      (E) unmodified
s3_client_roundtrip              15 passed            5.27 s      (E) unmodified
s3_client_upload_peers           12 passed            3.07 s      (E) unmodified
```

No `FAILED` or `panicked` anywhere in the log. `git diff --binary HEAD` after the run has the same sha256 as
`patch.diff` (`1e475446…5cef1`), so the gate changed no tracked file.

Items 1 and 4 have no red of their own: item 1 is documentation (checked sentence by sentence against the code
above), and item 4 is fixture hygiene whose assertions are unchanged.

## Refuting my own tests

- **(a) Genuine red? Yes.** Red A fails the new probe budget test on all 14 over-budget legs and no others; red B
  fails the gateway CR test and the `line-endings` case at the `keys_without_a_row` assertion. Each mutation is the
  exact behaviour the sign-off named or the invariant forbids. A full revert of the production change does not
  build, the case the brief pre-declares.
- **(b) Production path? Yes.** Every leg drives the production `S3Client::probe` or `S3Client::delete_objects`
  through the real `aws-sdk-s3` orchestrator, the client's own connector, `Capture`/`read_whole` and
  `delete_rows`/`DeleteResult`, against a real TCP endpoint on loopback: the in-process Wyrd gateway for the CR test,
  the scripted endpoint for the rest. No mock, no copy.
- **(c) Fixture includes the fault? Yes.** The CR test puts and deletes keys that really hold a CR and a CRLF through
  the real gateway, whose real `xml_escape` writes them raw (the red B output shows the rows came back as
  `cr\nkey` and `crlf\nkey`); the scripted case sends a raw CR, a raw CRLF and a `&#13;` in `<Key>`; the budget legs
  send 65,537 bytes (with a length and chunked) and exactly 65,536.

## Commit-readiness

- `cargo fmt --all -- --check`: clean (I ran `cargo fmt --all` after each edit; it reflowed two lines of the new
  test). `typos` over the three touched files: clean. Both also ran inside the green gate.
- Clippy with `-D warnings` runs inside `cargo xtask ci`; see the green run.
- `patch.diff` is `git diff --binary HEAD` on the worktree, and it reverse-applies (`git apply --check -R`). The test
  file is also copied to the bundle as `s3_client_smoke_verbs.rs`, byte-identical to the worktree's.
- Scratch: my logs and the diff comparisons are in `$PDCA_SCRATCH` (`pdca-builder-956-it6-*`), left for the harness
  to reclaim, per the builder's filesystem rule.

## External dependencies

`typos` and `docs-renderer`, as the brief lists. No Docker, no network beyond `127.0.0.1`.
No NEEDS-HUMAN external-dependency item.

## For your attention at sign-off

1. **`response.rs:44-45` changed beyond the lines you named** (item 1, above): the first half of the sentence whose
   second half you named. Revert is one line if you want it exact.
2. **The CR test depends on the gateway's bug** (`crates/gateway-s3/src/lib.rs:3803`, out of scope by your
   instruction). When the gateway escapes the CR, the test fails on purpose and the `document.rs` sentence needs its
   "as the Wyrd gateway writes it" clause removed. The scripted `line-endings` case does not depend on the gateway.
3. **The architecture doc is unchanged.** Item 2 names `document.rs` only, and the doc's bulk-delete text does not
   claim the keys are compared exactly. If you want the line-ending rule there too, it is one clause after "with the
   requested keys no row names and the rows that name no requested key reported beside them, not judged".
4. **The probe budget test covers seven probes, not one**, and pins that set (item 3, above). If you would rather
   have the one probe you asked for, dropping the loop to `GetBucketLocation` and the pin is a ten-line edit.
5. Carried over, unchanged: `head_object`'s panic on an overlong key is #955's and still untracked; C5 will still
   show `fail` for the three known timeouts in `s3_client_nonconforming_responses` (brief (E) forbids changing it),
   with 0 missed; the low-severity notes from iteration 3's adversary were left alone.
