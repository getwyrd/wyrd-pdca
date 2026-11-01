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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 392 of 392 instrumentable changed lines executed (floor 80%); 392 of 715 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 58 mutants tested in 3m: 56 caught, 2 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.49s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #667 fix prevents encoded multipart selectors from reaching destructive S3 object routes in an independent red→green run; an intermittent test failure, prior-art confirmation, and fitness sign-off remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Exact refusal statuses, error codes, and object-survival checks make the routing-only outcome falsifiable; subsequent carry-forward instructions authorize the docs and encoded bulk-delete corrections (`brief.md:23`, `brief.md:131`, `brief.md:137`). |
| C2 Reproduction (red pre-fix) | PASS | Independently reverting production reproduced PUT 200 with overwritten bytes and DELETE 204 followed by HEAD/GET 404; 11 tests compiled, with 7 assertion failures and 4 passes (`reviewer-evidence/red.log:300`, `reviewer-evidence/red.log:381`; discriminator at `target/crates/server/tests/s3_multipart_routing.rs:617`). |
| C3 Change | PASS | Multipart selectors cannot reach the ordinary method table, and bulk delete retains explicit refusals after their removal from the denylist; the restored target matches the supplied patch (`target/crates/gateway-s3/src/lib.rs:602`, `target/crates/gateway-s3/src/lib.rs:775`, `target/crates/gateway-s3/src/lib.rs:2031`, `reviewer-evidence/scanners.log:10`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the intermittent access-log assertion requires correction or a recorded flake disposition — the routing red→green is confirmed, but one broader-suite failure remains unexplained despite clean isolated, parallel, serial, and baseline reruns (`target/crates/gateway-s3/src/lib.rs:4919`, `reviewer-evidence/gateway-tests.log:129`, `reviewer-evidence/green.log:32`, `reviewer-evidence/gateway-retry.log:105`). |
| C5 Causal adequacy | PASS | Matching the once-decoded signed keys removes the routing mismatch itself; duplicate/shape refusals and double-encoding controls protect against both destructive fall-through and over-decoding, with no capability-probe workaround (`target/crates/gateway-s3/src/lib.rs:478`, `target/crates/gateway-s3/src/lib.rs:616`, `target/crates/server/tests/s3_multipart_routing.rs:760`). |
| T1 Structure | PASS | Classification stays inside the protocol gateway, with typed operation arguments and one replacement arm per future verb; no backend dependency or trait expansion is introduced (`target/crates/gateway-s3/src/lib.rs:516`, `target/crates/gateway-s3/src/lib.rs:2256`). |
| T2 Shape | PASS | Canonical digit parsing, duplicate refusal, and explicit method/path matching preserve the required distinction between malformed multipart requests and unsupported part reads (`target/crates/gateway-s3/src/lib.rs:578`, `target/crates/gateway-s3/src/lib.rs:616`, `target/crates/gateway-s3/src/lib.rs:636`). |
| T3 Runtime | PASS | The real wire fixture demonstrates object survival after refused requests and actual deletion in the bulk-delete control; exchanges are bounded and fixture drop aborts the server (`target/crates/server/tests/s3_multipart_routing.rs:189`, `target/crates/server/tests/s3_multipart_routing.rs:381`, `target/crates/server/tests/s3_multipart_routing.rs:130`, `target/crates/server/tests/s3_multipart_routing.rs:878`). |
| T4 Contribution | NEEDS-HUMAN | Confirm an affected-path search of merged history and closed/rejected work — the supplied target has one synthetic commit and no remotes, and the brief's prior-attempt summary does not establish that search, so earlier dispositions cannot be checked (`reviewer-evidence/prior-art.log:1`, `reviewer-evidence/prior-art.log:10`, `brief.md:116`). |
| T5 Judgment | PASS | No additional implementation finding: the earlier framing and task-ownership defects have executable regressions, and the docs/comments now accurately distinguish routing from still-raw listing parameters (`target/crates/server/tests/s3_multipart_routing.rs:530`, `target/crates/server/tests/s3_multipart_routing.rs:594`, `target/docs/design/architecture/05-building-block-view.md:139`, `target/crates/server/tests/multipart_gateway_seam.rs:77`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that safe routing/refusal is the intended release outcome — the tests establish protection against accidental object mutation, while all six multipart operations intentionally remain unavailable until #668–#670 (`target/crates/gateway-s3/src/lib.rs:2256`, `target/crates/server/tests/s3_multipart_routing.rs:703`). |

The routing evidence is independently reproduced. In the supplied disposable target, I stashed only `crates/gateway-s3/src/lib.rs`, retained the new test, and ran `cargo test --offline -p wyrd-server --test s3_multipart_routing`. The pre-fix run compiled and failed by assertion. After restoring production, the same 11 tests passed, together with all 7 existing `multipart_gateway_seam` tests (`reviewer-evidence/green.log:15`, `reviewer-evidence/green.log:32`). The target is restored to the supplied patch, and reverse-apply validation succeeds; there is no target-state caveat.

The broader-suite anomaly remains unclassified, not a confirmed patch defect. The first `cargo test --offline -p wyrd-gateway-s3 --lib` run passed 96 tests and failed the existing `a_bodyless_response_is_recorded_complete_not_aborted` assertion because its capture was empty (`reviewer-evidence/gateway-tests.log:130`). That test passed alone; all 97 tests subsequently passed both in parallel and serially (`reviewer-evidence/gateway-bodyless-isolated.log:7`, `reviewer-evidence/gateway-retry.log:105`, `reviewer-evidence/gateway-serial.log:104`). Thirty pre-fix runs also passed: five individual logs, `reviewer-evidence/gateway-base-1.log` through `gateway-base-5.log`, and 25 attempts in `reviewer-evidence/gateway-base-repeat.log`. These results do not establish a host fault or a pre-existing flake, so C4 requests a disposition without attributing the failure to this patch.

The declared external dependency is discharged: real `typos` ran successfully, along with `cargo fmt --all -- --check`, docs lint, and patch/whitespace checks (`reviewer-evidence/scanners.log:1`). Docs rendering produced 101 pages and passed its link audit (`reviewer-evidence/docs-render.log:2`). The in-process fixture can exhibit the forbidden failure, as the red run demonstrates; it is not a substitute topology that makes data loss impossible.

The frozen gate results below are adjudicated from their captured output, separately from the independent reruns above. Instance-scoped wrappers are not supplied in the target; full CI, coverage, mutation, batch review, and TiKV feature compilation were not independently repeated.

| Gate | Verdict | Evidence |
|------|---------|----------|
| C4-ci | PASS | The frozen log records spelling/docs checks, fmt, clippy, build/tests, dependency checks, conformance and DST, ending with all checks passed (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:4513`); this does not erase the independent test anomaly above. |
| C4-verify | PASS | Captured 11 green tests and 7 assertion failures/4 passes with production reverted agree with the independent run (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:131`). |
| C4-diff-cov | PASS | Captured coverage is 392/392 instrumentable changed lines; 323 changed lines are unscored, and the log explicitly describes generous treatment of co-located test lines (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:196`, `gate-logs/C4-diff-cov.log:209`). |
| C5-mutants | PASS | Captured unmutated baseline passes; 58 mutants yield 56 caught and 2 unviable, with none reported missed (`gate-logs/C5-mutants.log:10`). |
| T4-batch-review | PASS | Captured batch result reports zero blocking findings; it supplies no affected-path prior-art search (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | Contribution artifacts are intentionally drafted after Check; the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Captured clippy runs compile the TiKV backend and server feature selections successfully; this is compilation evidence, not a live TiKV integration test (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`). |

The prior-art investigation checked all four changed paths and found only the synthetic base commit (`reviewer-evidence/prior-art.log:7`). The brief documents previous #508 attempts and #491, but merged and closed/rejected repository work is unavailable in this artifact-only snapshot. T4 therefore concerns missing historical evidence; the publish-time contribution deferral is N/A and creates no human action.

<!-- pdca:leaf-complete -->

### Advisory — adversary

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

### Advisory — code-review

- No introduced correctness bugs found in the classifier and dispatch (`crates/gateway-s3/src/lib.rs:602`, `crates/gateway-s3/src/lib.rs:1968`, `crates/gateway-s3/src/lib.rs:2227`) or the new test harness (`crates/server/tests/s3_multipart_routing.rs:126`, `crates/server/tests/s3_multipart_routing.rs:273`, `crates/server/tests/s3_multipart_routing.rs:400`).
- No actionable reuse, simplification, or efficiency findings. The classifier uses fixed slots in one pass (`crates/gateway-s3/src/lib.rs:610`), and shared response helpers keep bucket and object routing consistent (`crates/gateway-s3/src/lib.rs:2227`). The separate multipart arms are intentional extension points required by the brief (`crates/gateway-s3/src/lib.rs:2256`).

Reviewed the supplied diff against the read-only target and frozen gate evidence; did not rerun builds. The evidence records passing CI, 11 integration tests passing with the fix and 7 assertion failures without it, and 56 caught / 2 unviable mutants with none surviving. No NEEDS-HUMAN findings.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether the intermittent access-log assertion requires correction or a recorded flake disposition — the routing red→green is confirmed, but one broader-suite failure remains unexplained despite clean isolated, parallel, serial, and baseline reruns (`target/crates/gateway-s3/src/lib.rs:4919`, `reviewer-evidence/gateway-tests.log:129`, `reviewer-evidence/green.log:32`, `reviewer-evidence/gateway-retry.log:105`).
- [ ] T4 Contribution — Confirm an affected-path search of merged history and closed/rejected work — the supplied target has one synthetic commit and no remotes, and the brief's prior-attempt summary does not establish that search, so earlier dispositions cannot be checked (`reviewer-evidence/prior-art.log:1`, `reviewer-evidence/prior-art.log:10`, `brief.md:116`).
- [ ] Validation — fitness-to-purpose — Confirm that safe routing/refusal is the intended release outcome — the tests establish protection against accidental object mutation, while all six multipart operations intentionally remain unavailable until #668–#670 (`target/crates/gateway-s3/src/lib.rs:2256`, `target/crates/server/tests/s3_multipart_routing.rs:703`).
- [ ] `crates/gateway-s3/src/lib.rs:1982`: the patch opens one more spelling of a bucket-alias bulk delete. A signed `POST /b%2Fa?d%65lete` with `<Key>k</Key>` answered `200` and deleted object `b/a/k`. The original code refused that spelling with `501`. The plain spelling `POST /b%2Fa?delete` deletes `b/a/k` on both trees, so the defect is older than this patch and any client can already reach it. Brief item (6) only covers multipart routes, and the brief never asked for the encoded marker to reach bulk delete (that came from the iteration 2 carry-forward). **Decision needed:** either add the `invalid_bucket_name` check to this route, which is one line now that `bucket_name` is in scope, or decline it here with a tracking issue, as the rubric's out-of-scope rule says. This does not block the fix.
- [ ] `gate-logs/T4-batch-review.log:10`, `check-gates.json:111`: I can't confirm the T4 pass from its evidence. This is the gate that failed in rounds 1 and 2. It is described as three codex review passes, but it finished in 11.02 seconds on a 100 KB patch and its log holds only the `0 blocking` summary line. That could be a cached result or a review that never really ran; I can't tell which, and `review-batch.md` is not in my inputs. Please open that file before relying on this pass. This says nothing against the code.
- [ ] **The bulk-delete marker is still matched raw, which falls short of the brief's invariant.** See `crates/gateway-s3/src/lib.rs:1996` and `:720`. The brief says a form the gateway implements is reachable by exactly one route whatever the spelling, and it lists bulk delete among the routes that read the one classification. But `POST /b?d%65lete` with a valid `<Delete>` body signs as `delete`, yet answers `501 NotImplemented` instead of running DeleteObjects. The patch documents this at `lib.rs:718-719`, and `op_label` (`lib.rs:320`) depends on the same raw predicate. The request fails closed, so no data is lost and no SDK sends that spelling. The decision is one of two: (a) accept this and narrow the invariant's wording, or (b) decode the marker in both `dispatch` and `op_label` together. Option (b) widens a destructive path, so this is a judgment call, not a builder fix.
- [ ] **UploadPartCopy will hit the UploadPart arm, and #669 is told to change "nothing else".** A `PUT /b/k?partNumber=1&uploadId=U` with an `x-amz-copy-source` header is classified `Multipart(UploadPart)` at `lib.rs:2059`. That happens before the CopyObject refusal at `lib.rs:2077`, which only the plain PUT arm reaches. Today the arm answers 501, so nothing breaks. But the doc comment at `lib.rs:2281` says #668–#670 each replace their arm "and nothing else", and `multipart_response` (`lib.rs:2282`) receives no headers, body or state. So #669 has to widen that signature anyway. If it does not also carry a copy-source refusal, an UploadPartCopy (empty body) would be stored as an empty part. That is the same data-loss shape `lib.rs:2065-2070` warns about. Either add this to #669's brief, or have the classifier refuse `x-amz-copy-source` on UploadPart now.
- [ ] `crates/gateway-s3/src/lib.rs:1999` passes the raw query to `list_objects`, and `query_param` (`crates/gateway-s3/src/lib.rs:828`) still matches raw keys. An encoded listing parameter is therefore silently ignored. On the fixed tree, `GET /b?list-type=3` answers `400` but `GET /b?list%2Dtype=3` answers `200` with a V1 listing. `GET /b?pref%69x=zzz` lists every object where `prefix=zzz` lists none, `max%2Dkeys=0` is ignored, `list%2Dtype=2` returns the V1 shape, and `encoding%2Dtype=bogus` skips its `400`. The base tree does the same, so the patch did not cause this. But it breaks the invariant at `brief.md:75-76` ("whatever spelling the client uses"), which is the argument round 2 used to pull `d%65lete` into this patch. Decide whether listing parameters belong here or in a tracked issue; under the rubric's out-of-scope rule, I would file an issue.
- [ ] `crates/gateway-s3/src/lib.rs:1984` lets a bucket alias reach bulk delete under a new spelling. `POST /b%2Fa?d%65lete` with a body naming `k` answered `501` on the base. On the fixed tree it answers `200`, deletes `b/a/k`, and reports the deleted key as `k`. The plain `POST /b%2Fa?delete` already did this on the base, so the patch adds a second spelling, not a new power. The patch refuses this alias on the multipart routes (`crates/gateway-s3/src/lib.rs:2229`) and the PUT arm keeps its refusal (`crates/gateway-s3/src/lib.rs:2073`), but the bulk route does not use it. No test covers the alias on bulk delete. `brief.md:48-54` limits the alias rule to multipart shapes, so extending it is a scope decision. Refusing it would change the plain spelling's behaviour too.
- [ ] `check-gates.json:105-108` records T4 as a pass with "0 blocking, 0 recorded-rejected, 0 noise-dropped". The oracle is described as three codex passes, but `gate-logs/T4-batch-review.log:6` shows 18.43 seconds for a diff of about 98 KB, and the log has no per-pass output. Earlier rounds of this bundle found 4 and then 2 blocking items. The result may be correct or served from a cache, but the log does not show that three passes reviewed this diff. I was not given `review-batch.md`, so I could not check. T4 gates acceptance, so someone should open that file and confirm it.

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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide whether the intermittent access-log assertion requires correction or a recorded flake disposition — the routing red→green is confirmed, but one broader-suite failure remains unexplained despite clean isolated, parallel, serial, and baseline reruns (`target/crates/gateway-s3/src/lib.rs:4919`, `reviewer-evidence/gateway-tests.log:129`, `reviewer-evidence/green.log:32`, `reviewer-evidence/gateway-retry.log:105`).; T4 Contribution — Confirm an affected-path search of merged history and closed/rejected work — the supplied target has one synthetic commit and no remotes, and the brief's prior-attempt summary does not establish that search, so earlier dispositions cannot be checked (`reviewer-evidence/prior-art.log:1`, `reviewer-evidence/prior-art.log:10`, `brief.md:116`).. Deferred, not addressed here: 2 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
