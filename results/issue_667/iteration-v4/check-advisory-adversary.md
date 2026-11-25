# Adversarial review: issue #667, S3 multipart routing

**I could not refute the fix.** I re-ran the test on both trees and tried about 30 extra request forms over the wire. No multipart spelling reached a plain verb or a listing. What's left is one scope decision, one gate result I can't verify from its log, and three small wording problems.

## Findings for a human

- NEEDS-HUMAN [human] — `crates/gateway-s3/src/lib.rs:1982`: the patch opens one more spelling of a bucket-alias bulk delete. A signed `POST /b%2Fa?d%65lete` with `<Key>k</Key>` answered `200` and deleted object `b/a/k`. The original code refused that spelling with `501`. The plain spelling `POST /b%2Fa?delete` deletes `b/a/k` on both trees, so the defect is older than this patch and any client can already reach it. Brief item (6) only covers multipart routes, and the brief never asked for the encoded marker to reach bulk delete (that came from the iteration 2 carry-forward). **Decision needed:** either add the `invalid_bucket_name` check to this route, which is one line now that `bucket_name` is in scope, or decline it here with a tracking issue, as the rubric's out-of-scope rule says. This does not block the fix.
- NEEDS-HUMAN [human] — `gate-logs/T4-batch-review.log:10`, `check-gates.json:111`: I can't confirm the T4 pass from its evidence. This is the gate that failed in rounds 1 and 2. It is described as three codex review passes, but it finished in 11.02 seconds on a 100 KB patch and its log holds only the `0 blocking` summary line. That could be a cached result or a review that never really ran; I can't tell which, and `review-batch.md` is not in my inputs. Please open that file before relying on this pass. This says nothing against the code.

## Smaller problems (no rebuild needed for these alone)

- `crates/gateway-s3/src/lib.rs:319`: this comment, which the patch rewrote, says the `delete` metric label "can never name a request the bulk handler did not take". That is false. `POST /b?delete&uploads` gets the label, but `lib.rs:1974` refuses it with `501` before the handler runs. Refused requests are therefore counted in the `delete` metrics. Fix the wording when the commit is next touched.
- `check-gates.json:48`, `gate-logs/C4-verify.log:134`: the gate says "11 test(s) ran red", but the log's own result at `gate-logs/C4-verify.log:131` is 4 passed, 7 failed. Seven is the right number and matches the test file's header. The red-to-green proof still holds, but the count is wrong, which looks like a bug in the `run-verify.sh` harness rather than in this diff.
- `brief.md:87`: the claim that #668–#670 can "replace their arm and nothing else" is not quite true. `query_route_response` (`lib.rs:2227`) and `multipart_response` (`lib.rs:2256`) are not async and receive no state, key, headers or body, so the first verb slice must also change both call sites (`lib.rs:1994`, `lib.rs:2032`). The comment at `lib.rs:2253` admits this. It is a small cost, and no change is needed now.
- `docs/design/proposals/draft/0017-blackbox-validation-tool.md:245`: this draft still calls #491 a live defect and gives the correct answer as `501`. This form now answers `400` (`PUT /b/k?part%4Eumber=1`, as the brief requires). The draft's known-open entry should be updated when #491 closes, and any test vector built from it should not expect `501` here.

## Attempts that failed to refute the fix

- **Red and green runs, `crates/server/tests/s3_multipart_routing.rs:1`:** I re-ran both on a scratch copy.
  - The fixed code passed all 11 tests. The original `lib.rs` with the test kept failed the same 7 tests by assertion, with no build error.
  - On the original code, `PUT ?part%4Eumber=1` answered `200` and overwrote the object, per the frozen `C4-verify` log, which also shows `DELETE ?upload%49d=…` answering `204` and deleting it. My own probe on the original code confirmed the same delete with other spellings.
  - The test uses the production `serve_s3` and a real `Gateway`, not a copy.
  - Its signing shares `canonical_query` with the verifier, so encoded keys truly pass authentication.
  - The object check compares ETag and bytes after every row, and `/b/a/k` is seeded for the alias rows, so a misrouted request would show up.
- **Differences between decoders, `crates/gateway-s3/src/sigv4.rs:195` and `lib.rs:3779`:** the two functions have the same logic, including malformed `%` and `+` handling. Lossy UTF-8 conversion happens in both. I found no spelling that signs as a multipart key but routes as something else.
- **Encoded forms, `lib.rs:607`:** these were all classified correctly:
  - `%75pload%49%64=U`
  - valueless `upload%49d`
  - `uploadId=` with an empty value
  - `partNumber=%31`, read as 1
  - `partNumber=%C2%B9`, refused with `400`
  - `POST /b?delete&%70artNumber`, refused with `501`
- **SDK and path forms, `lib.rs:636`:** `x-id` beside a shape keeps its route, and `/b/?uploads` is treated as the bucket route. `//b/k?uploadId=U` answers `501`. On the alias, `/b%2F/k` and lowercase `%2f` are refused, while `/b%252Fa/k` correctly is not.
- **Forms that still reach the plain verb, `docs/design/architecture/05-building-block-view.md:139`:** `DELETE /b/k?uploadid=U` answers `204` and deletes the object. `PUT /b/k?partNumber%3D1%26uploadId%3DU` answers `200` and overwrites it. `DELETE /b/k?uploadId%3DU` answers `204` and deletes it. All three behave the same on the original code. They sign as different literal keys, since S3 keys are case-sensitive and `%3D` is part of the key, so I don't count them as refutations. "No spelling" in the doc means percent-encoded spellings only.
- **Listing parameters:** `GET /b?list%2Dtype=3` still answers `200`. Round 3 already raised this, the doc now states it, and the earlier deferral stands, so I am not raising it again.

<!-- pdca:leaf-complete -->
