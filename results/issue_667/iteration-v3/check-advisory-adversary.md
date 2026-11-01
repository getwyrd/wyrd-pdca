# Adversarial review: issue #667, multipart routing classifier

**Result: I could not break the fix for the three multipart keys.** Five smaller findings remain. None of them shows a multipart key reaching a plain verb or a listing.

**How I tested.** I built a scratch copy of the target and re-ran the proof. With the fix, 11 of 11 tests pass. With `crates/gateway-s3/src/lib.rs` reverted, 7 fail by assertion and 4 pass. I also sent 39 extra signed requests through `serve_s3` on both trees, using a probe test that is not part of the patch.

## Findings

- NEEDS-HUMAN [human] — `crates/gateway-s3/src/lib.rs:1999` passes the raw query to `list_objects`, and `query_param` (`crates/gateway-s3/src/lib.rs:828`) still matches raw keys. An encoded listing parameter is therefore silently ignored. On the fixed tree, `GET /b?list-type=3` answers `400` but `GET /b?list%2Dtype=3` answers `200` with a V1 listing. `GET /b?pref%69x=zzz` lists every object where `prefix=zzz` lists none, `max%2Dkeys=0` is ignored, `list%2Dtype=2` returns the V1 shape, and `encoding%2Dtype=bogus` skips its `400`. The base tree does the same, so the patch did not cause this. But it breaks the invariant at `brief.md:75-76` ("whatever spelling the client uses"), which is the argument round 2 used to pull `d%65lete` into this patch. Decide whether listing parameters belong here or in a tracked issue; under the rubric's out-of-scope rule, I would file an issue.

- NEEDS-HUMAN [impl] — `crates/gateway-s3/src/lib.rs:1950-1952` says "no spelling of a key reaches a route its plain spelling cannot", and `docs/design/architecture/05-building-block-view.md:139` says "every route reads that decoded form". The `list%2Dtype=3` case above disproves both on the fixed tree: the encoded form reaches a listing and the plain form is refused. The patch added both sentences. Narrow them to what is true: route choice reads the decoded keys, while the listing's own parameters are still matched raw. If the first finding is fixed in this PR instead, this one goes away.

- NEEDS-HUMAN [human] — `crates/gateway-s3/src/lib.rs:1984` lets a bucket alias reach bulk delete under a new spelling. `POST /b%2Fa?d%65lete` with a body naming `k` answered `501` on the base. On the fixed tree it answers `200`, deletes `b/a/k`, and reports the deleted key as `k`. The plain `POST /b%2Fa?delete` already did this on the base, so the patch adds a second spelling, not a new power. The patch refuses this alias on the multipart routes (`crates/gateway-s3/src/lib.rs:2229`) and the PUT arm keeps its refusal (`crates/gateway-s3/src/lib.rs:2073`), but the bulk route does not use it. No test covers the alias on bulk delete. `brief.md:48-54` limits the alias rule to multipart shapes, so extending it is a scope decision. Refusing it would change the plain spelling's behaviour too.

- NEEDS-HUMAN [impl] — `crates/server/tests/multipart_gateway_seam.rs:77-78` is now wrong, and the patch did not touch it. It says the named subresource is "the FIRST denylisted query key" reported by `unsupported_subresource`. The multipart keys have left that denylist, and these six forms now get their name from `MultipartOp::selector` (`crates/gateway-s3/src/lib.rs:550`). The test still passes because the names happen to match. This is a one-comment fix.

- NEEDS-HUMAN [human] — `check-gates.json:105-108` records T4 as a pass with "0 blocking, 0 recorded-rejected, 0 noise-dropped". The oracle is described as three codex passes, but `gate-logs/T4-batch-review.log:6` shows 18.43 seconds for a diff of about 98 KB, and the log has no per-pass output. Earlier rounds of this bundle found 4 and then 2 blocking items. The result may be correct or served from a cache, but the log does not show that three passes reviewed this diff. I was not given `review-batch.md`, so I could not check. T4 gates acceptance, so someone should open that file and confirm it.

## Notes that need no decision

- `check-gates.json:48` says "11 test(s) ran red", which overstates the result. `gate-logs/C4-verify.log` and my re-run both show 7 failed and 4 passed. The 4 are expected: legs 4 and 7 are regression guards, and the other 2 test the harness. The proof holds; only the wording is off.
- `docs/design/proposals/draft/0017-blackbox-validation-tool.md:245-248` says #491 is live on every build and that the correct answer is `501`. After this patch, `PUT ?part%4Eumber=1` answers `400`, so a test vector written from that text would fail. This is a draft proposal, not the living architecture doc, so the docs-currency rule does not clearly apply.

## Attacks that did not refute the fix

- **Evidence.** The wire test drives the real `serve_s3`, the real SigV4 check and a real `Gateway`. The red log shows real damage on the base: `k` overwritten with the request's body, `k` deleted with `204`. No assertion in the seven red tests passes for the wrong reason that I could find. The harness tests fail on torn responses and on a server left running.
- **Decoder mismatch.** `percent_decode_utf8` (`crates/gateway-s3/src/lib.rs:3781`) and `sigv4::percent_decode` (`crates/gateway-s3/src/sigv4.rs:195`) use the same logic, and `canonical_query` splits parameters the same way, so routing and signing see the same keys.
- **Encoded multipart keys.** Every encoded key I tried was refused and left `k` intact: fully encoded `%75%70%6c%6f%61%64%49%64=U`, mixed `upload%49%64=U`, `&&uploadI%64=U&&`, `%70artNumber=1&uploadId=U`.
- **Odd values.** `partNumber=%31` is a well-formed UploadPart (`501`). `partNumber=1%00`, `partNumber%3D1` and `partNumber=1;uploadId=U` answer `400`. Empty `uploadId` routes to Abort (`501`), never the plain DELETE.
- **Other shapes.** `HEAD` and `OPTIONS` with a multipart key answer `400`. `/b/?uploads` routes as the bucket form and `DELETE /b/?uploadId=U` answers `400`. The alias with a denylisted key answers `501` naming that key, which follows leg 5; nothing is written.
- **Error text.** The client's method name reaches the `InvalidArgument` message, but `error_response` XML-escapes it.
- **Forms that still reach the plain verb, unchanged from the base.** `DELETE /b/k` with `UPLOADID=U`, `uploadId%20=U`, `uploadId%FF=U`, `a=1;uploadId=U` or `?uploadId=U` still deletes `k` (`204`), and `PUT /b/k?PartNumber=1` still overwrites it (`200`). I did not count these as refutations because each signs as a different key, not a spelling of a multipart key. I did not check how real S3 treats them. The test's mixed-case gap is deliberate only if query keys are case-sensitive, which is how the gateway treats them today.

<!-- pdca:leaf-complete -->
