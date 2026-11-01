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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 348 of 348 instrumentable changed lines executed (floor 80%); 348 of 624 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 66 mutants tested in 7m: 17 missed, 47 caught, 2 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 30.03s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #667 multipart-routing fix reproduces red→green for encoded-query data loss; two test-harness defects still need builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The contract distinguishes malformed requests, supported plain requests, and valid unsupported operations using exact responses and object survival; `brief.md:20`, `target/crates/server/tests/s3_multipart_routing.rs:252`. |
| C2 Reproduction (red pre-fix) | PASS | Independent pre-fix execution produced five assertion failures among seven tests, including PUT 200 with overwrite and DELETE 204 with removal; `reviewer-evidence/red.log:289`, `reviewer-evidence/red.log:354`. |
| C3 Change | PASS | Encoded selectors cannot reach either plain dispatch or bulk deletion through the former raw-key gap; the changes stay within the two authorized files; `target/crates/gateway-s3/src/lib.rs:805`, `target/crates/gateway-s3/src/lib.rs:2021`, `target/crates/gateway-s3/src/lib.rs:2059`. |
| C4 Verification (red→green) | PASS | Independent red→green and instrumented runs pass; frozen full CI passes, while local full CI fails only because the disposable snapshot lacks its advisory-db gitlink; `reviewer-evidence/green.log:15`, `reviewer-evidence/ci.log:3875`, `gate-logs/C4-ci.log:4510`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Require the response oracle to reject incomplete chunked framing — its current success on a missing terminal CRLF can certify a truncated response; `target/crates/server/tests/s3_multipart_routing.rs:434`, `reviewer-evidence/probes.log:4`. |
| T1 Structure | PASS | Routing decisions remain within the S3 gateway's existing dependency boundary, with explicit operation variants and centralized refusals; `target/crates/gateway-s3/src/lib.rs:520`, `target/crates/gateway-s3/src/lib.rs:2282`. |
| T2 Shape | PASS | Canonical decimal parsing and duplicate rejection meet the required grammar; the 72,840-byte two-file patch remains below the 100 KB backstop; `target/crates/gateway-s3/src/lib.rs:589`, `target/crates/gateway-s3/src/lib.rs:628`, `brief.md:114`. |
| T3 Runtime | FAIL | Fixture teardown violates the abort-on-drop rule: the listener still accepts connections after its owner and temporary directory are dropped; `target/crates/server/tests/s3_multipart_routing.rs:130`, `target/AGENTS.md:196`, `reviewer-evidence/probes.log:8`. |
| T4 Contribution | FAIL | The batch review contains two distinct, reproduced rubric defects that remain unresolved; path-based prior-art checking is complete, and the contribution-artifact audit is N/A until publish; `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | NEEDS-HUMAN [impl] | Make fixture ownership stop the spawned server before releasing its storage — dropping its JoinHandle currently detaches a task that outlives its resources; `target/crates/server/tests/s3_multipart_routing.rs:109`, `target/crates/server/tests/s3_multipart_routing.rs:130`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the routing-only refusal contract for this milestone and acknowledge the snapshot audit caveat — clients remain protected while actual multipart service awaits #668–#670; `brief.md:3`, `brief.md:92`, `reviewer-evidence/ci.log:3877`. |

Two distinct findings explain the implementation verdicts; the batch review's three framing reports are duplicates, not three separate defects.

1. **Reject an incomplete chunked terminator (C5).** At `target/crates/server/tests/s3_multipart_routing.rs:434`, `decode_chunked` returns immediately after reading the zero-size chunk line. A diagnostic compiled from the supplied test source accepted `1\r\nx\r\n0\r\n`, an unterminated trailer, and garbage after the last chunk, each as `Ok([120])` (`reviewer-evidence/probes.log:3`). A complete XML error or object body can therefore satisfy the response assertions despite truncated framing. Validate the terminal CRLF/trailer section and add a regression that rejects incomplete termination. This is the explicit protocol-input requirement at `target/AGENTS.md:176`. The defect grounds at line 434; two batch entries instead cite unrelated lines 449/461.

2. **Abort the server when its fixture is dropped (T3/T5).** `Served` owns the temporary directory but no task handle (`target/crates/server/tests/s3_multipart_routing.rs:109`); `start` discards the handle returned by `tokio::spawn` at line 130. The diagnostic starts the real fixture, drops it, yields, and successfully connects to its listener (`reviewer-evidence/probes.log:8`). The existing per-test Tokio runtime eventually stops the task, so this is a fixture-lifetime defect, not a demonstrated production-server leak. Retain an abort-on-drop owner and stop the helper before releasing its directory, as required by `target/AGENTS.md:196`.

The production change addresses the demonstrated cause: routing now consumes the same once-decoded key identity used by signing. The independent red run actually overwrote and deleted the seeded object, so this fixture can exhibit the forbidden failure. The patch introduces no optional-capability probe or fallback that triggers the C5 symptom-guard smell-test. The C5 implementation concern above concerns the test oracle's framing fidelity.

All source citations ground in the supplied `$PDCA_TARGET` (`target/`). Its base identifies `376b106248d2ac2e816c5d258609e04de09a425f`; reverse-applying the patch with `--check` succeeds. I stashed only the production change, retained the added test for red, restored the stash, and reran green. The original two-file patch is restored and the stash is empty (`reviewer-evidence/stash.log:1`, `reviewer-evidence/target-state.log:1`). No implementation changes were made.

| Gate | Assessment | Evidence |
|------|------------|----------|
| C4-verify | PASS, independently reproduced | `cargo test --offline --locked -p wyrd-server --test s3_multipart_routing`: red exit 101, 2 passed/5 assertion failures; green exit 0, 7 passed. `reviewer-evidence/red.log:354`, `reviewer-evidence/green.log:15`; corroborated by `gate-logs/C4-verify.log:104`. |
| C4-ci | PASS in frozen evidence; local snapshot caveat | Local fmt, typos, clippy, build, conformance, and DST checks completed. The workspace test failure and dependency-audit failure both stem from the missing advisory-db gitlink, documented below. The original full gate finishes successfully at `gate-logs/C4-ci.log:4510`. |
| C4-diff-cov | PASS in frozen evidence; instrumented tests independently reproduced | The frozen wrapper scores 348/348 instrumentable changed lines, with 276 unscored (`gate-logs/C4-diff-cov.log:224`). Independent instrumentation passed 7 wire and 95 gateway tests (`reviewer-evidence/coverage-rerun.log:276`, `reviewer-evidence/coverage-rerun.log:402`); all 299 changed lines with counted, non-gap LLVM segment starts were executed, including 154 before the co-located tests (`reviewer-evidence/coverage-summary.txt:1`). That cross-check uses a different denominator from the wrapper. |
| C5-mutants | FAIL in frozen scanner; survivors require individual interpretation | The full log reports 66 tested, 17 missed, 47 caught, 2 unviable (`gate-logs/C5-mutants.log:30`). Independent mutation enumeration reproduced the same 66 candidates (`reviewer-evidence/mutants-list.log:1`); the full execution campaign was not rerun. The log does not establish 17 distinct defects: several response/name mutations contradict explicit wire assertions at `target/crates/server/tests/s3_multipart_routing.rs:264`, while diagnostic-text mutations exceed the specified exact-code contract. |
| T4-batch-review | FAIL, two unique findings confirmed | Three reports describe the same terminal-CRLF omission; the fourth describes the detached helper. Both were independently exercised above. `gate-logs/T4-batch-review.log:10`. |
| T4-contribution | N/A | Contribution artifacts are absent by design at Check; the substantive audit must rerun at publish. `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS from frozen compilation evidence | Both requested feature-enabled clippy commands finish successfully, at `gate-logs/host-tikv.log:112` and `gate-logs/host-tikv.log:211`; this is compilation evidence, not a live TiKV service test. |

The local full-CI failure is a **target-state caveat for sign-off**, not a patch defect. `git ls-files --stage .github/advisory-db` returns no entry in this self-contained snapshot. Consequently, the existing index test fails at `target/xtask/tests/advisory_db.rs:66`, and the dependency audit cannot obtain its pin (`reviewer-evidence/ci.log:3229`, `reviewer-evidence/ci.log:3877`). The frozen original-worktree log shows the actual pinned database and successful dependency audits (`gate-logs/C4-ci.log:3861`). No index repair, substituted checker, or shim was used. The brief's external dependency, `typos`, ran successfully both locally and in the frozen gate (`reviewer-evidence/ci.log:2`, `gate-logs/C4-ci.log:11`). Original instance-scoped wrappers and their missing presence in this snapshot are not findings; their supplied logs support the assessments above.

The **prior-art check ran by affected file path**. GitHub history at the stated base returned 40 commits for `crates/gateway-s3/src/lib.rs`, including the bulk-delete denylist repair and multipart seam; the new test path returned zero commits (`reviewer-evidence/prior-art-merged.log:1`, `reviewer-evidence/prior-art-test-history.log:1`). I also inspected the complete changed-file inventories of all 19 returned closed, unmerged PRs: none touches either affected path (`reviewer-evidence/prior-art-closed.json:2`). The rejected #508 routing cases enumerated in `brief.md:116` are represented by the current classifier/wire tests, including double decoding, signed decimal values, duplicates, bucket fall-through, and part-ranged reads. No settled tracked-issue deferral was re-raised.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #667 (multipart routing classifier)

**Bottom line: I tried to refute the fix and could not.** The red→green proof reproduces, the test drives the production path, and every edge input I fed the real classifier agrees with how SigV4 signs it. Below are the attempts, then two scope questions for a human and some notes on the gate verdicts.

## Attempts that failed to refute

- **Re-ran the proof myself** (`crates/server/tests/s3_multipart_routing.rs:282`). I used a scratch copy of `$PDCA_TARGET`. With the patch applied, 7 of 7 tests pass. With `crates/gateway-s3/src/lib.rs` reverted to base and the test kept, 5 fail and 2 pass. All 5 failures are assertion panics at `:282`, none are build errors. The 2 passing legs, `valid_non_multipart_forms_are_unaffected` (`:560`) and `bulk_delete_keeps_refusing_a_multipart_selector` (`:675`), are the regression guards the brief says must be green on both trees. This matches `gate-logs/C4-verify.log`.
- **Production path, not a copy.** The test serves through `wyrd_server::cli::serve_s3` over a real `Gateway` (`s3_multipart_routing.rs:130-140`). Requests go over raw TCP and pass the production `sigv4::verify` (`lib.rs:1938`). Every row checks exact status and code. After every row, every seeded object is checked by HEAD ETag and GET bytes (`:168-195`), so a clobber or a delete cannot pass unseen. The test signs with the production `sigv4::sign` (`sigv4.rs:629`), which shares the verifier's canonicaliser. That is fine here: a real SDK that signs `partNumber=1` produces the same canonical string as the wire form `part%4Eumber=1`.
- **Looked for a decode mismatch between the classifier and SigV4.** `DecodedQuery::decode` (`lib.rs:474-489`) splits on `&` and the first `=`, exactly like `canonical_query` (`sigv4.rs:252-261`). Its decoder `percent_decode_utf8` (`lib.rs:3805-3823`) is byte-for-byte the same algorithm as `sigv4::percent_decode` (`sigv4.rs:195-213`): no `+`-as-space, malformed escapes kept literally, lossy UTF-8. I ran 14 edge inputs through the real `classify_query` in a throwaway probe test and printed each beside its SigV4 canonical form. In every case the classifier's key matched the key the client signed:
  - `uploadI%64=U&partNumber=1` → UploadPart
  - `partNumber=%31&uploadId=U` → part 1
  - `partNumber=1%` → IllFormed
  - full-width `%EF%BC%91` → IllFormed
  - bare `?uploadId` → Abort with an empty id, never the plain DELETE
  - `?uploadId%3DU` → None (it signs as the parameter name `uploadId=U`, which is not a multipart key, matching S3's own single decode)
  - `POST` bucket `?uploads` → IllFormed
  - `OPTIONS` object `?uploadId=U` → IllFormed

  I found no spelling that signs as a multipart key but reaches a plain verb.
- **Double decoding.** `PUT ?part%254Eumber=1` is deliberately a plain PUT (`lib.rs:5595`, `s3_multipart_routing.rs:592`). That is correct, not a bypass: the client signed the literal key `part%4Eumber`.
- **Other readers of the raw query.** After the classifier, the object arms never read the query: the only `&query` uses are `lib.rs:1941`, `:1979`, `:1996` and `:2025`. `list_objects` still reads raw keys through `query_param` (`lib.rs:854-860`), but only for listing parameters, and only once the classifier has returned `None`. That code is unchanged and outside this brief.
- **Strict `partNumber` grammar** (`lib.rs:589-596`). It rejects sign, leading zero, whitespace, a fraction, non-ASCII digits and values above `u32::MAX`. This meets the rubric's grammar-strictness rule. Leaving the range check to the seam is what the brief asks for.
- **Docs currency.** No living architecture doc (`docs/design/architecture/*.md`) describes the subresource denylist, so the rule is not triggered.

## Findings for a human

- NEEDS-HUMAN [human] — **The bulk-delete marker is still matched raw, which falls short of the brief's invariant.** See `crates/gateway-s3/src/lib.rs:1996` and `:720`. The brief says a form the gateway implements is reachable by exactly one route whatever the spelling, and it lists bulk delete among the routes that read the one classification. But `POST /b?d%65lete` with a valid `<Delete>` body signs as `delete`, yet answers `501 NotImplemented` instead of running DeleteObjects. The patch documents this at `lib.rs:718-719`, and `op_label` (`lib.rs:320`) depends on the same raw predicate. The request fails closed, so no data is lost and no SDK sends that spelling. The decision is one of two: (a) accept this and narrow the invariant's wording, or (b) decode the marker in both `dispatch` and `op_label` together. Option (b) widens a destructive path, so this is a judgment call, not a builder fix.
- NEEDS-HUMAN [human] — **UploadPartCopy will hit the UploadPart arm, and #669 is told to change "nothing else".** A `PUT /b/k?partNumber=1&uploadId=U` with an `x-amz-copy-source` header is classified `Multipart(UploadPart)` at `lib.rs:2059`. That happens before the CopyObject refusal at `lib.rs:2077`, which only the plain PUT arm reaches. Today the arm answers 501, so nothing breaks. But the doc comment at `lib.rs:2281` says #668–#670 each replace their arm "and nothing else", and `multipart_response` (`lib.rs:2282`) receives no headers, body or state. So #669 has to widen that signature anyway. If it does not also carry a copy-source refusal, an UploadPartCopy (empty body) would be stored as an empty part. That is the same data-loss shape `lib.rs:2065-2070` warns about. Either add this to #669's brief, or have the classifier refuse `x-amz-copy-source` on UploadPart now.

## On the gate verdicts (no action from me)

- **C5-mutants "fail", 17 survivors.** Most of them are an artefact of which tests ran. cargo-mutants (a tool that injects small code changes to see whether any test notices) seems to have run only `wyrd-gateway-s3`'s own tests, not the wire test in `wyrd-server`. By reading the code, the wire test kills these mutants: the `bucket.contains('/')` guard turned `true` or `false` (`lib.rs:2270`, caught by legs 2 and 6), `query_route_response` and `multipart_not_implemented` replaced by `Default::default()` (`lib.rs:2256`, `lib.rs:2296`, caught by every leg), and `MultipartOp::name` (`lib.rs:542`, caught by the `.naming(...)` checks in legs 1 and 3). The survivors that would also pass the wire test only change message text: `MultipartOp::selector` (`lib.rs:555`) and the `multipart_shapes` arms (`lib.rs:693`). The `&&`→`||` survivor at `lib.rs:1996` is on an unchanged line. None of these are behaviour gaps.
- **T4-batch-review, 4 blocking findings.** They point at `decode_chunked` (`s3_multipart_routing.rs:424-447`, not checking the final CRLF) and at the discarded server `JoinHandle` (`:130`). I checked whether either could make a red case read green. Neither can. By the time the zero-size chunk arrives, every body byte is already in hand. Every assertion is on status, code or message. `#[tokio::test]` drops its runtime at test end, which cancels the idle server task. The gate is still gating, so the builder must fix or formally reject them. I add no weight beyond that.
- **Overstated claim in `check-gates.json`.** The C4-verify `path_line` says "7 test(s) ran red". The log shows 7 ran, 5 failed and 2 passed on the red leg. The 2 that passed are the brief's designed regression guards, so the proof is sound, but the summary overstates it.
- **Draft doc drift.** `docs/design/proposals/draft/0017-blackbox-validation-tool.md:246-248` expects the #491 encoded-`partNumber` vector to answer `501`. With this patch, `PUT ?part%4Eumber=1` without `uploadId` answers `400 InvalidArgument`, as brief (1) requires. If someone writes that blackbox vector to the doc's text, it will read red against a fixed gateway. The doc is a draft and the brief made this choice deliberately, so I'm noting it, not flagging it.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/tests/s3_multipart_routing.rs:434`: `decode_chunked` returns success immediately after the zero-size chunk line. A response with the expected body followed by `0\r\n` and EOF therefore passes despite missing the terminal CRLF; incomplete trailers are likewise ignored. Validate the trailer section and its terminating empty line before returning. The existing decoder at `crates/server/tests/multipart_gateway_seam.rs:575` already handles this and could be factored into shared test support.

- NEEDS-HUMAN [impl] — `crates/server/tests/s3_multipart_routing.rs:130`: the server's `JoinHandle` is discarded, so dropping `Served` removes its chunk-store directory without cancelling the server that owns that store and listener. Cancellation is left to runtime teardown, contrary to the rubric's abort-on-drop requirement. Retain the handle in `Served` and abort it in `Drop`, following `crates/server/tests/s3_key_length_limit.rs:118`.

- NEEDS-HUMAN [impl] — `crates/gateway-s3/src/lib.rs:619`: optional efficiency/simplification: classification scans the decoded parameters separately for each multipart key, collects all matching values into temporary vectors, then builds another vector of present keys. Only presence, the first value, and duplicate counts are needed. Fixed slots populated in one pass would avoid these intermediate collections; build the key-name list only for the malformed-shape error. This is a cleanup opportunity, not a routing correctness defect.

No production routing correctness regression found. Assessment used the target source and frozen gate evidence; gates were not rerun.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Require the response oracle to reject incomplete chunked framing — its current success on a missing terminal CRLF can certify a truncated response; `target/crates/server/tests/s3_multipart_routing.rs:434`, `reviewer-evidence/probes.log:4`.
- [ ] T5 Judgment — Make fixture ownership stop the spawned server before releasing its storage — dropping its JoinHandle currently detaches a task that outlives its resources; `target/crates/server/tests/s3_multipart_routing.rs:109`, `target/crates/server/tests/s3_multipart_routing.rs:130`.
- [ ] Validation — fitness-to-purpose — Approve the routing-only refusal contract for this milestone and acknowledge the snapshot audit caveat — clients remain protected while actual multipart service awaits #668–#670; `brief.md:3`, `brief.md:92`, `reviewer-evidence/ci.log:3877`.
- [ ] **The bulk-delete marker is still matched raw, which falls short of the brief's invariant.** See `crates/gateway-s3/src/lib.rs:1996` and `:720`. The brief says a form the gateway implements is reachable by exactly one route whatever the spelling, and it lists bulk delete among the routes that read the one classification. But `POST /b?d%65lete` with a valid `<Delete>` body signs as `delete`, yet answers `501 NotImplemented` instead of running DeleteObjects. The patch documents this at `lib.rs:718-719`, and `op_label` (`lib.rs:320`) depends on the same raw predicate. The request fails closed, so no data is lost and no SDK sends that spelling. The decision is one of two: (a) accept this and narrow the invariant's wording, or (b) decode the marker in both `dispatch` and `op_label` together. Option (b) widens a destructive path, so this is a judgment call, not a builder fix.
- [ ] **UploadPartCopy will hit the UploadPart arm, and #669 is told to change "nothing else".** A `PUT /b/k?partNumber=1&uploadId=U` with an `x-amz-copy-source` header is classified `Multipart(UploadPart)` at `lib.rs:2059`. That happens before the CopyObject refusal at `lib.rs:2077`, which only the plain PUT arm reaches. Today the arm answers 501, so nothing breaks. But the doc comment at `lib.rs:2281` says #668–#670 each replace their arm "and nothing else", and `multipart_response` (`lib.rs:2282`) receives no headers, body or state. So #669 has to widen that signature anyway. If it does not also carry a copy-source refusal, an UploadPartCopy (empty body) would be stored as an empty part. That is the same data-loss shape `lib.rs:2065-2070` warns about. Either add this to #669's brief, or have the classifier refuse `x-amz-copy-source` on UploadPart now.
- [ ] `crates/server/tests/s3_multipart_routing.rs:434`: `decode_chunked` returns success immediately after the zero-size chunk line. A response with the expected body followed by `0\r\n` and EOF therefore passes despite missing the terminal CRLF; incomplete trailers are likewise ignored. Validate the trailer section and its terminating empty line before returning. The existing decoder at `crates/server/tests/multipart_gateway_seam.rs:575` already handles this and could be factored into shared test support.
- [ ] `crates/server/tests/s3_multipart_routing.rs:130`: the server's `JoinHandle` is discarded, so dropping `Served` removes its chunk-store directory without cancelling the server that owns that store and listener. Cancellation is left to runtime teardown, contrary to the rubric's abort-on-drop requirement. Retain the handle in `Served` and abort it in `Drop`, following `crates/server/tests/s3_key_length_limit.rs:118`.
- [ ] `crates/gateway-s3/src/lib.rs:619`: optional efficiency/simplification: classification scans the decoded parameters separately for each multipart key, collects all matching values into temporary vectors, then builds another vector of present keys. Only presence, the first value, and duplicate counts are needed. Fixed slots populated in one pass would avoid these intermediate collections; build the key-name list only for the malformed-shape error. This is a cleanup opportunity, not a routing correctness defect.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Require the response oracle to reject incomplete chunked framing — its current success on a missing terminal CRLF can certify a truncated response; `target/crates/server/tests/s3_multipart_routing.rs:434`, `reviewer-evidence/probes.log:4`.; T5 Judgment — Make fixture ownership stop the spawned server before releasing its storage — dropping its JoinHandle currently detaches a task that outlives its resources; `target/crates/server/tests/s3_multipart_routing.rs:109`, `target/crates/server/tests/s3_multipart_routing.rs:130`.; `crates/server/tests/s3_multipart_routing.rs:434`: `decode_chunked` returns success immediately after the zero-size chunk line. A response with the expected body followed by `0\r\n` and EOF therefore passes despite missing the terminal CRLF; incomplete trailers are likewise ignored. Validate the trailer section and its terminating empty line before returning. The existing decoder at `crates/server/tests/multipart_gateway_seam.rs:575` already handles this and could be factored into shared test support.; `crates/server/tests/s3_multipart_routing.rs:130`: the server's `JoinHandle` is discarded, so dropping `Served` removes its chunk-store directory without cancelling the server that owns that store and listener. Cancellation is left to runtime teardown, contrary to the rubric's abort-on-drop requirement. Retain the handle in `Served` and abort it in `Drop`, following `crates/server/tests/s3_key_length_limit.rs:118`.; `crates/gateway-s3/src/lib.rs:619`: optional efficiency/simplification: classification scans the decoded parameters separately for each multipart key, collects all matching values into temporary vectors, then builds another vector of present keys. Only presence, the first value, and duplicate counts are needed. Fixed slots populated in one pass would avoid these intermediate collections; build the key-name list only for the malformed-shape error. This is a cleanup opportunity, not a routing correctness defect.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b. Deferred, not addressed here: 2 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
