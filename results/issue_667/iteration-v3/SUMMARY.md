# Result — issue 667 / multipart-routing-classifier

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the object route refuses the multipart keys by matching **raw** query keys
  (`unsupported_subresource`, `crates/gateway-s3/src/lib.rs:466`, called at `:1842`), while
  SigV4 canonicalisation decodes and re-encodes them. So on `main`, `PUT /b/k?part%4Eumber=1`
  passes the guard, reaches the plain PUT arm, answers **200 and overwrites `k`**, and
  `DELETE /b/k?upload%49d=U` reaches the plain DELETE by the same raw match and deletes `k`. The residual is
  documented at `:455-465` as acceptable only because the raw key was denylisted (issue #491).
  The slices that route the verbs (#668–#670) need one classifier that sees every spelling, and
  they must not each reinvent it.
- Success criterion: the NEW file `crates/server/tests/s3_multipart_routing.rs` passes. It
  drives an in-process gateway over the wire (hand-signed raw requests, because an SDK will not
  spell these forms), on a bucket `b` holding an object `k` with known bytes. Every assertion is
  **exact status + exact S3 error code**, and after **every** request `k` answers HEAD `200`
  with its bytes unchanged:
  - **(1) Percent-encoded spellings classify as their decoded key.** `PUT /b/k?part%4Eumber=1`
    → `400 InvalidArgument` (it lacks `uploadId`; see (2)); `DELETE /b/k?upload%49d=0123…`
    (32 lowercase hex) → `501 NotImplemented`; `POST /b/k?upload%73` → `501 NotImplemented`.
  - **(2) Ill-formed multipart shapes answer `400 InvalidArgument`**, never a 2xx and never the
    plain verb: `PUT /b/k?partNumber=1` (no `uploadId`); `PUT /b/k?uploadId=U` (no
    `partNumber`); `PUT /b/k?uploads`; `partNumber` that is not canonical decimal digits —
    `abc`, `+1`, `01`, empty, `1.0`; a multipart key **repeated** (`?uploadId=U&uploadId=V`,
    `?partNumber=1&partNumber=2&uploadId=U`), refused rather than first-wins; `GET
    /b?uploadId=U` and `GET /b?partNumber=1` on the bucket route (v7 let these fall through to a
    `<ListBucketResult>`).
  - **(3) Well-formed shapes of the six verbs answer `501 NotImplemented` naming the operation**
    (unchanged wire behaviour until #668–#670): `POST /b/k?uploads` (Create), `PUT
    /b/k?partNumber=1&uploadId=U` (UploadPart), `POST /b/k?uploadId=U` (Complete), `DELETE
    /b/k?uploadId=U` (Abort) with a well-formed, an unknown and a malformed (`zz`) id,
    `GET /b/k?uploadId=U` (ListParts), `GET /b?uploads` (ListMultipartUploads). The DELETE forms
    are the point: a DELETE carrying `uploadId` in any spelling never reaches the object DELETE.
  - **(4) A valid S3 form that is not multipart stays as it is.** `GET /b/k?partNumber=1` and
    `HEAD /b/k?partNumber=1` (part-ranged GetObject, which this gateway does not implement)
    answer `501 NotImplemented`, not `400` (the v7 regression). An ordinary GET, a PUT with
    `x-id=PutObject`, and a ListObjectsV2 are unaffected.
  - **(5) A still-denylisted subresource wins, in any spelling.** `PUT
    /b/k?partNumber=1&uploadId=U&t%61gging=1` and `GET /b?uploads&%61cl` answer `501
    NotImplemented` naming `tagging` / `acl`.
  - **(6) A decoded bucket containing `/` is refused on every multipart shape.** The path is
    split on its first literal `/` before decoding (`:1832-1834` region), so `/b%2Fa/k` and
    `/b/a/k` produce the same flat name. `POST /b%2Fa/k?uploads`, `PUT
    /b%2Fa/k?partNumber=1&uploadId=U`, `POST /b%2Fa/k?uploadId=U`, `DELETE
    /b%2Fa/k?uploadId=U` and `GET /b%2Fa/k?uploadId=U` each answer **`400 InvalidBucketName`**,
    the answer the PUT arm gives today (`:1885-1893`). The rule is that the alias is refused
    before any session read, target match or core call, on every multipart route.
  - **(7) Bulk delete keeps refusing a multipart selector.** `POST /b?delete` is intercepted
    before the bucket guard (`:1766-1788`) and refuses denylisted keys through
    `foreign_subresource_on_delete` (`:578`), which reads `UNSUPPORTED_SUBRESOURCES` (`:410`).
    With a well-formed `<Delete>` body naming `k` and a second object `k2`: `POST
    /b?delete&uploads`, `POST /b?delete&uploadId=U` and `POST /b?delete&part%4Eumber=1` each
    answer `501 NotImplemented` naming the key, and both objects still answer HEAD `200`. This
    already holds on `main`, so it is a regression guard: green on both trees.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: `crates/gateway-s3/src/lib.rs` only, and the new test. Decode the query once into a
  typed classification that every route reads (object PUT/GET/HEAD/DELETE/POST, bucket
  GET/POST, bulk delete): a multipart shape, ill-formed or well-formed, carries the operation
  and its parsed `uploadId` / `partNumber`. Remove `uploads`, `uploadId` and `partNumber` from
  `UNSUPPORTED_SUBRESOURCES` **only together with** an explicit bulk-delete refusal of all three
  (7), since that function reads the same list. Every well-formed arm answers `501
  NotImplemented` naming the operation, in one place each, so that #668–#670 each replace their
  arm and nothing else. Parse `partNumber` as canonical digits (no sign, no leading zero, no
  whitespace) per the grammar-strictness rule (`../wyrd/AGENTS.md`, *Protocol input*). The
  numeric **range** (`1..=10000`) is not checked here; the seam's `upload_part` contract runs it
  first (`crates/gateway-core/src/lib.rs:710-834`), and #669 asserts it. / out of scope:
  serving any verb; the composition-root impl (`crates/server/src/lib.rs:799-890`); the target
  check (each verb child checks its own); `crates/gateway-core`; the remaining denylist entries
  and both guards' behaviour for them; docs (no API operation is added).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 392 of 392 instrumentable changed lines executed (floor 80%); 392 of 717 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 58 mutants tested in 3m: 56 caught, 2 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.72s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No remaining implementation defect found in #667's multipart-routing fix: encoded selectors must never overwrite/delete an object or fall through to a listing; fitness-to-purpose remains a human decision.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract distinguishes malformed shapes, valid unsupported operations, ordinary requests and object survival, making the data-loss claim falsifiable; `brief.md:21`, `target/crates/server/tests/s3_multipart_routing.rs:281`. |
| C2 Reproduction (red pre-fix) | PASS | Independently reproduced PUT `200` with overwritten bytes and DELETE `204` followed by HEAD `404`; 7 of 11 tests failed by assertion on the pre-fix tree; `reviewer-evidence/red-green.log:287`, `reviewer-evidence/red-green.log:368`. |
| C3 Change | PASS | The amended scope covers routing, its regression tests and the requested architecture correction; the carry-forward fixes are present and the patch reverse-checks against the supplied target; `brief.md:131`, `target/docs/design/architecture/05-building-block-view.md:139`, `reviewer-evidence/bundle-integrity.log:1`. |
| C4 Verification (red→green) | PASS | The same wire suite compiled on both trees and passed all 11 tests after restoring the fix; 97 gateway unit tests also passed independently; full-CI replay caveat is detailed below; `reviewer-evidence/red-green.log:404`, `reviewer-evidence/checks.log:138`. |
| C5 Causal adequacy | PASS | Routing now agrees with SigV4's once-decoded keys, removing the destructive fall-through cause; duplicate, malformed and double-encoded forms are distinguished without a capability probe; `target/crates/gateway-s3/src/lib.rs:482`, `target/crates/gateway-s3/src/lib.rs:606`, `target/crates/server/tests/s3_multipart_routing.rs:768`. |
| T1 Structure | PASS | Classification remains inside the protocol adapter and exposes separate operation arms for the later verb slices, preserving the narrow gateway seam; `target/crates/gateway-s3/src/lib.rs:606`, `target/crates/gateway-s3/src/lib.rs:2258`. |
| T2 Shape | PASS | Canonical decimal parsing and duplicate refusal make multipart selection unambiguous; the 98,516-byte patch remains below the brief's 100 KB backstop; `target/crates/gateway-s3/src/lib.rs:584`, `target/crates/gateway-s3/src/lib.rs:620`, `brief.md:114`, `reviewer-evidence/bundle-integrity.log:4`. |
| T3 Runtime | PASS | Real-gateway requests prove refused destructive forms preserve ETags and bytes; fixture exchanges are bounded and the server is aborted on drop; `target/crates/server/tests/s3_multipart_routing.rs:197`, `target/crates/server/tests/s3_multipart_routing.rs:134`, `target/crates/server/tests/s3_multipart_routing.rs:389`. |
| T4 Contribution | PASS | Living documentation matches the 400/501 contract, affected-path merged and rejected prior art was checked, and the frozen batch review reports no blocking findings; `target/docs/design/architecture/05-building-block-view.md:139`, `reviewer-evidence/prior-art.log:3`, `gate-logs/T4-batch-review.log:10`. |
| T5 Judgment | PASS | The earlier framing, fixture-lifetime and encoded-delete concerns are discharged by executable regressions; no further builder-fixable concern was found; `target/crates/server/tests/s3_multipart_routing.rs:538`, `target/crates/server/tests/s3_multipart_routing.rs:602`, `target/crates/server/tests/s3_multipart_routing.rs:904`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this routing-only milestone while multipart service remains unavailable until #668–#670 — the data-loss fix is exercised, but clients still cannot complete multipart uploads; `target/crates/gateway-s3/src/lib.rs:2258`, `target/docs/design/architecture/05-building-block-view.md:139`. |

The independent red→green run used `git stash push` to remove the tracked fix while retaining the new test, then `git stash pop` to restore it. Both legs ran `cargo test --offline --locked -p wyrd-server --test s3_multipart_routing -- --nocapture`. The pre-fix result was 4 passed / 7 assertion failures; the restored result was 11 passed. The patch still reverse-applies cleanly and no review changes were made to its source.

The full-CI replay has a **target-state caveat**, not a patch defect: the harness's synthetic Git index omits `.github/advisory-db`. The unchanged index test therefore fails and the advisory wall cannot read its pin (`reviewer-evidence/ci.log:3236`, `target/xtask/tests/advisory_db.rs:65`). The frozen gate actually used pin `40f5be6fb12ed93fc591bf4ab23b7d793b3f2239`, passed all three dependency scans, and completed CI (`gate-logs/C4-ci.log:3869`, `gate-logs/C4-ci.log:3880`, `gate-logs/C4-ci.log:4516`). Independent `typos`, formatting, docs lint/render, gateway unit tests and patch checks passed (`reviewer-evidence/checks.log:1`); the brief's `typos` dependency was exercised.

Additional gate dispositions distinguish independent reruns from frozen evidence:

- **C4-diff-cov — PASS:** 392/392 instrumentable changed lines executed; 325 other changed lines were unscored, so this is not a claim of coverage for every patch line (`gate-logs/C4-diff-cov.log:156`, `gate-logs/C4-diff-cov.log:169`).
- **C5-mutants — PASS:** the independent replay with `--cap-lints true` tested all 58 diff mutants: 56 caught, two unviable, no survivors or timeouts, matching the frozen result. The two unviable replacements require nonexistent `Default` implementations; the earlier uncapped attempt was discounted because lint failures prevented 29 additional mutants from reaching tests (`reviewer-evidence/mutants-capped.log:5`, `gate-logs/C5-mutants.log:13`).
- **T4-batch-review — PASS:** the supplied log records zero blocking findings; this review independently checked routing, test fidelity and the standing rubric (`gate-logs/T4-batch-review.log:10`).
- **T4-contribution — N/A:** contribution artifacts are absent by design at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).
- **host-tikv — PASS from frozen evidence:** both the backend and server feature selections completed real Clippy builds (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`).

The prior-art check queried merged history by each of the three affected paths and examined all 19 closed/unmerged PRs by their changed-file paths, with no truncated lists or matching rejected PR. Issue #508 records the seven rejected broader attempts; their routing counterexamples are covered here, and the interaction with #491 was checked (`reviewer-evidence/prior-art.log:1`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

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

### Advisory — code-review

No introduced correctness defects found. The response-framing and server-lifetime issues from the prior round are addressed. Frozen evidence records 11 passing wire tests, seven assertion failures with production reverted, and no surviving mutants; builds were not rerun for this review.

- Optional efficiency — `crates/gateway-s3/src/lib.rs:322`: `op_label` constructs a complete `DecodedQuery`, allocating and decoding every key and value merely to detect `delete`. An authenticated bucket-scoped POST then constructs the same decoded query again at `crates/gateway-s3/src/lib.rs:1953`. Consider reusing the decoded representation between labeling and dispatch, or sharing a lazy decoded-key iterator so labeling avoids materializing unused values. This is a cleanup opportunity, not a routing correctness defect.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Accept this routing-only milestone while multipart service remains unavailable until #668–#670 — the data-loss fix is exercised, but clients still cannot complete multipart uploads; `target/crates/gateway-s3/src/lib.rs:2258`, `target/docs/design/architecture/05-building-block-view.md:139`.
- [ ] `crates/gateway-s3/src/lib.rs:1999` passes the raw query to `list_objects`, and `query_param` (`crates/gateway-s3/src/lib.rs:828`) still matches raw keys. An encoded listing parameter is therefore silently ignored. On the fixed tree, `GET /b?list-type=3` answers `400` but `GET /b?list%2Dtype=3` answers `200` with a V1 listing. `GET /b?pref%69x=zzz` lists every object where `prefix=zzz` lists none, `max%2Dkeys=0` is ignored, `list%2Dtype=2` returns the V1 shape, and `encoding%2Dtype=bogus` skips its `400`. The base tree does the same, so the patch did not cause this. But it breaks the invariant at `brief.md:75-76` ("whatever spelling the client uses"), which is the argument round 2 used to pull `d%65lete` into this patch. Decide whether listing parameters belong here or in a tracked issue; under the rubric's out-of-scope rule, I would file an issue.
- [ ] `crates/gateway-s3/src/lib.rs:1950-1952` says "no spelling of a key reaches a route its plain spelling cannot", and `docs/design/architecture/05-building-block-view.md:139` says "every route reads that decoded form". The `list%2Dtype=3` case above disproves both on the fixed tree: the encoded form reaches a listing and the plain form is refused. The patch added both sentences. Narrow them to what is true: route choice reads the decoded keys, while the listing's own parameters are still matched raw. If the first finding is fixed in this PR instead, this one goes away.
- [ ] `crates/gateway-s3/src/lib.rs:1984` lets a bucket alias reach bulk delete under a new spelling. `POST /b%2Fa?d%65lete` with a body naming `k` answered `501` on the base. On the fixed tree it answers `200`, deletes `b/a/k`, and reports the deleted key as `k`. The plain `POST /b%2Fa?delete` already did this on the base, so the patch adds a second spelling, not a new power. The patch refuses this alias on the multipart routes (`crates/gateway-s3/src/lib.rs:2229`) and the PUT arm keeps its refusal (`crates/gateway-s3/src/lib.rs:2073`), but the bulk route does not use it. No test covers the alias on bulk delete. `brief.md:48-54` limits the alias rule to multipart shapes, so extending it is a scope decision. Refusing it would change the plain spelling's behaviour too.
- [ ] `crates/server/tests/multipart_gateway_seam.rs:77-78` is now wrong, and the patch did not touch it. It says the named subresource is "the FIRST denylisted query key" reported by `unsupported_subresource`. The multipart keys have left that denylist, and these six forms now get their name from `MultipartOp::selector` (`crates/gateway-s3/src/lib.rs:550`). The test still passes because the names happen to match. This is a one-comment fix.
- [ ] `check-gates.json:105-108` records T4 as a pass with "0 blocking, 0 recorded-rejected, 0 noise-dropped". The oracle is described as three codex passes, but `gate-logs/T4-batch-review.log:6` shows 18.43 seconds for a diff of about 98 KB, and the log has no per-pass output. Earlier rounds of this bundle found 4 and then 2 blocking items. The result may be correct or served from a cache, but the log does not show that three passes reviewed this diff. I was not given `review-batch.md`, so I could not check. T4 gates acceptance, so someone should open that file and confirm it.
- [ ] **The bulk-delete marker is still matched raw, which falls short of the brief's invariant.** See `crates/gateway-s3/src/lib.rs:1996` and `:720`. The brief says a form the gateway implements is reachable by exactly one route whatever the spelling, and it lists bulk delete among the routes that read the one classification. But `POST /b?d%65lete` with a valid `<Delete>` body signs as `delete`, yet answers `501 NotImplemented` instead of running DeleteObjects. The patch documents this at `lib.rs:718-719`, and `op_label` (`lib.rs:320`) depends on the same raw predicate. The request fails closed, so no data is lost and no SDK sends that spelling. The decision is one of two: (a) accept this and narrow the invariant's wording, or (b) decode the marker in both `dispatch` and `op_label` together. Option (b) widens a destructive path, so this is a judgment call, not a builder fix.
- [ ] **UploadPartCopy will hit the UploadPart arm, and #669 is told to change "nothing else".** A `PUT /b/k?partNumber=1&uploadId=U` with an `x-amz-copy-source` header is classified `Multipart(UploadPart)` at `lib.rs:2059`. That happens before the CopyObject refusal at `lib.rs:2077`, which only the plain PUT arm reaches. Today the arm answers 501, so nothing breaks. But the doc comment at `lib.rs:2281` says #668–#670 each replace their arm "and nothing else", and `multipart_response` (`lib.rs:2282`) receives no headers, body or state. So #669 has to widen that signature anyway. If it does not also carry a copy-source refusal, an UploadPartCopy (empty body) would be stored as an empty part. That is the same data-loss shape `lib.rs:2065-2070` warns about. Either add this to #669's brief, or have the classifier refuse `x-amz-copy-source` on UploadPart now.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — `crates/gateway-s3/src/lib.rs:1950-1952` says "no spelling of a key reaches a route its plain spelling cannot", and `docs/design/architecture/05-building-block-view.md:139` says "every route reads that decoded form". The `list%2Dtype=3` case above disproves both on the fixed tree: the encoded form reaches a listing and the plain form is refused. The patch added both sentences. Narrow them to what is true: route choice reads the decoded keys, while the listing's own parameters are still matched raw. If the first finding is fixed in this PR instead, this one goes away.; `crates/server/tests/multipart_gateway_seam.rs:77-78` is now wrong, and the patch did not touch it. It says the named subresource is "the FIRST denylisted query key" reported by `unsupported_subresource`. The multipart keys have left that denylist, and these six forms now get their name from `MultipartOp::selector` (`crates/gateway-s3/src/lib.rs:550`). The test still passes because the names happen to match. This is a one-comment fix.. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
