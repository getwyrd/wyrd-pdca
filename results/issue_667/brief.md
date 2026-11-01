# s3: multipart routing — every spelling of uploads / uploadId / partNumber is classified, never mishandled (508.2)

> Do reads ONLY this file. First of four children of #508 (tracker #667). Nothing in this child
> serves a multipart verb: every well-formed multipart request still answers `501
> NotImplemented`. What changes is that no spelling of the three keys can reach a plain object
> PUT, GET, DELETE or a listing. Line numbers are at getwyrd/wyrd `origin/main` @ `376b106`
> (2026-10-30); re-locate them on your base.

- **Slug:** multipart-routing-classifier
- **Track:** alpha
- **Kind:** bug (data loss) + groundwork for #668–#670
- **Defect:** the object route refuses the multipart keys by matching **raw** query keys
  (`unsupported_subresource`, `crates/gateway-s3/src/lib.rs:466`, called at `:1842`), while
  SigV4 canonicalisation decodes and re-encodes them. So on `main`, `PUT /b/k?part%4Eumber=1`
  passes the guard, reaches the plain PUT arm, answers **200 and overwrites `k`**, and
  `DELETE /b/k?upload%49d=U` reaches the plain DELETE by the same raw match and deletes `k`. The residual is
  documented at `:455-465` as acceptable only because the raw key was denylisted (issue #491).
  The slices that route the verbs (#668–#670) need one classifier that sees every spelling, and
  they must not each reinvent it.
- **Success criterion:** the NEW file `crates/server/tests/s3_multipart_routing.rs` passes. It
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
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** on `main`, a signed `PUT /b/k?part%4Eumber=1` with a body answers `200`, and
  a GET of `k` returns the new body (the parent brief verified this form). By the same raw
  match, a signed `DELETE /b/k?upload%49d=0123456789abcdef0123456789abcdef` should answer `204`
  and delete `k`; confirm it on the RED leg and record what it answered in `build-notes.md`.
- **Falsifiability:** assertion-red, in-process, on `main` as it is. (1) reds on the 200/204 and
  on the object's survival; (2), (3)'s malformed id, and (6) red on their exact status (`main`
  answers `501` or a 2xx, never the required `400`); (4)'s `GET ?partNumber` and (5) and (7) are
  regression guards, green on both trees. The test file references **nothing this child adds**
  (no new symbol, no new dev-dependency): `run-verify.sh` reverts production and keeps the added
  test, and a test that does not compile there is UNVERIFIABLE (exit 77). Record in
  `build-notes.md` how many tests ran and failed on the RED leg, and that the failures were
  assertions.
- **Invariant to restore:** a request form this gateway does not implement is refused, never
  silently mishandled, and a form it does implement is reachable by exactly one route, whatever
  spelling the client uses (`crates/gateway-s3/src/lib.rs:403-409` and `:455-465`, issue #507's adversary,
  issue #491; 0016's "the denylist loses its multipart entries in that slice", `0016:2718-2721`,
  that is, loses them to routing and not to a fall-through). This is a property of the
  dispatch table as a whole, which is why the test enumerates forms rather than testing one.
- **Scope:** `crates/gateway-s3/src/lib.rs` only, and the new test. Decode the query once into a
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
- **External dependencies:** `typos`
- **Test file:** `crates/server/tests/s3_multipart_routing.rs` (NEW)
- **Difficulty:** medium
- **Do model:** opus-max
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** wave 1 of #508's split; #668 (child-2) builds on it. No external edge: it
  needs only `main`. Do not add any after acceptance.
- **Verification posture:** DEFAULT, in-process, red before the fix and green after, at Check.
- **Production reach:** every request the S3 role serves passes through the classifier. That
  is the point: the test drives the real router, SigV4 verifier and gateway.
- **Citations expected:** cite `path:line` on your base for every change. Mirror:
  `crates/gateway-s3/src/lib.rs:403-470` (the denylist, why it exists, the raw-match residual),
  `:556` (`unsupported_subresource_decoded`, the decoded match the bucket route already uses),
  `:578` (`foreign_subresource_on_delete`), `:1766-1800` (bulk delete and the bucket guard),
  `:1842-1905` (the object guard, the copy-source refusal, the alias and key-length checks).
  Harness: salvage the in-process gateway, `seed_bucket` and the hand-signed raw-request helpers
  from `results/issue_508/s3_multipart_upload.rs:1-389` (this bundle's seventh attempt) and
  `crates/server/tests/s3_list_objects.rs:82`. Serve through `wyrd_server::cli::serve_s3`
  (`crates/server/src/cli.rs:2658`, `pub`). Size budget: about 45–60 KB; stay under the 100 KB
  backstop.
- **Prior-art check (triage cycles):** the routing findings of #508's seven attempts (v1 §9:
  four destructive fall-throughs and the malformed DELETE; v5 §9: double percent-decoding; v7
  §9: `GET ?partNumber` → 400; `iteration-v7/../review-batch.md`: `+1` accepted by
  `u32::from_str`, `GET /bucket?uploadId` falling through, first-wins duplicates) are each one
  of the assertions above. Interacting open issue: #491. No open PR touches the classifier.
- **Disposition hint:** likely-fix

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Require the response oracle to reject incomplete chunked framing — its current success on a missing terminal CRLF can certify a truncated response; `target/crates/server/tests/s3_multipart_routing.rs:434`, `reviewer-evidence/probes.log:4`.; T5 Judgment — Make fixture ownership stop the spawned server before releasing its storage — dropping its JoinHandle currently detaches a task that outlives its resources; `target/crates/server/tests/s3_multipart_routing.rs:109`, `target/crates/server/tests/s3_multipart_routing.rs:130`.; `crates/server/tests/s3_multipart_routing.rs:434`: `decode_chunked` returns success immediately after the zero-size chunk line. A response with the expected body followed by `0\r\n` and EOF therefore passes despite missing the terminal CRLF; incomplete trailers are likewise ignored. Validate the trailer section and its terminating empty line before returning. The existing decoder at `crates/server/tests/multipart_gateway_seam.rs:575` already handles this and could be factored into shared test support.; `crates/server/tests/s3_multipart_routing.rs:130`: the server's `JoinHandle` is discarded, so dropping `Served` removes its chunk-store directory without cancelling the server that owns that store and listener. Cancellation is left to runtime teardown, contrary to the rubric's abort-on-drop requirement. Retain the handle in `Served` and abort it in `Drop`, following `crates/server/tests/s3_key_length_limit.rs:118`.; `crates/gateway-s3/src/lib.rs:619`: optional efficiency/simplification: classification scans the decoded parameters separately for each multipart key, collects all matching values into temporary vectors, then builds another vector of present keys. Only presence, the first value, and duplicate counts are needed. Fixed slots populated in one pass would avoid these intermediate collections; build the key-name list only for the malformed-shape error. This is a cleanup opportunity, not a routing correctness defect.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b. Deferred, not addressed here: 2 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 66 mutants tested in 7m: 17 missed, 47 caught, 2 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Resolve #667's explicit docs exclusion against the docs-currency requirement—architecture still promises 501 for every multipart form, and the batch gate remains red for that mismatch; `brief.md:94`, `target/AGENTS.md:169`, `target/docs/design/architecture/05-building-block-view.md:137`, `gate-logs/T4-batch-review.log:10`.; `docs/design/architecture/05-building-block-view.md:137` still says "the S3 wire still refuses every multipart form with `501`". That is now false: `PUT /b/k?partNumber=1` answers `400 InvalidArgument` (`crates/gateway-s3/src/lib.rs:2035`, table row in `crates/server/tests/s3_multipart_routing.rs`, leg 2). The diff touches no docs, and this is the same gap behind the red T4 gate (`gate-logs/T4-batch-review.log:10`). The unwarranted claim is in `brief.md:94`, "docs (no API operation is added)". The AGENTS.md Docs currency rule also covers a change that *alters* an API operation, and calls it a merge requirement. Fix: correct that sentence in the same PR, stating that well-formed forms answer `501` and ill-formed shapes answer `400`.; `crates/gateway-s3/src/lib.rs:1972` still detects bulk delete on the raw query (`is_delete_subresource`, `:696`), so one spelling of an implemented form does not reach its route. Probe over the wire: `POST /b?delete` runs the bulk handler, but `POST /b?d%65lete`, which SigV4 signs as the same key, answers `501` naming `delete`. This contradicts two things:; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — `crates/gateway-s3/src/lib.rs:1950-1952` says "no spelling of a key reaches a route its plain spelling cannot", and `docs/design/architecture/05-building-block-view.md:139` says "every route reads that decoded form". The `list%2Dtype=3` case above disproves both on the fixed tree: the encoded form reaches a listing and the plain form is refused. The patch added both sentences. Narrow them to what is true: route choice reads the decoded keys, while the listing's own parameters are still matched raw. If the first finding is fixed in this PR instead, this one goes away.; `crates/server/tests/multipart_gateway_seam.rs:77-78` is now wrong, and the patch did not touch it. It says the named subresource is "the FIRST denylisted query key" reported by `unsupported_subresource`. The multipart keys have left that denylist, and these six forms now get their name from `MultipartOp::selector` (`crates/gateway-s3/src/lib.rs:550`). The test still passes because the names happen to match. This is a one-comment fix.. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide whether the intermittent access-log assertion requires correction or a recorded flake disposition — the routing red→green is confirmed, but one broader-suite failure remains unexplained despite clean isolated, parallel, serial, and baseline reruns (`target/crates/gateway-s3/src/lib.rs:4919`, `reviewer-evidence/gateway-tests.log:129`, `reviewer-evidence/green.log:32`, `reviewer-evidence/gateway-retry.log:105`).; T4 Contribution — Confirm an affected-path search of merged history and closed/rejected work — the supplied target has one synthetic commit and no remotes, and the brief's prior-attempt summary does not establish that search, so earlier dispositions cannot be checked (`reviewer-evidence/prior-art.log:1`, `reviewer-evidence/prior-art.log:10`, `brief.md:116`).. Deferred, not addressed here: 2 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
