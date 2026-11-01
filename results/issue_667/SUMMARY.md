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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.15s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

#667 review: prevent multipart query spellings from overwriting/deleting objects or reaching listings; independent red→green supports the fix, with fitness-to-purpose reserved for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The refusal contract distinguishes malformed shapes, six unsupported operations, part-ranged reads, and ordinary requests; the signed-wire cases make those boundaries falsifiable (`brief.md:20`, `target/crates/server/tests/s3_multipart_routing.rs:617`). |
| C2 Reproduction (red pre-fix) | PASS | Independent production-only stash reproduced PUT 200 with overwritten bytes and DELETE 204 followed by HEAD 404; all 11 tests compiled, with seven assertion failures (`reviewer-evidence/red.log:300`, `reviewer-evidence/red.log:381`). |
| C3 Change | PASS | Multipart selectors are intercepted before ordinary operations on both path types, including bulk-delete refusals; the living architecture now states the changed contract (`target/crates/gateway-s3/src/lib.rs:1970`, `target/crates/gateway-s3/src/lib.rs:1993`, `target/crates/gateway-s3/src/lib.rs:2031`, `target/docs/design/architecture/05-building-block-view.md:139`). |
| C4 Verification (red→green) | PASS | Restoring the exact patch made all 11 regressions pass; 53 neighboring integration tests, 97 gateway tests, scoped clippy, and prose/format checks also passed; the full-gate retry caveat is tracked in #988 (`reviewer-evidence/green.log:90`, `reviewer-evidence/gateway-tests.log:125`, `reviewer-evidence/clippy.log:226`, `reviewer-evidence/flake.log:2`). |
| C5 Causal adequacy | PASS | The authentication/routing key mismatch is removed at classification, including duplicate spellings and exactly-once decoding; no optional-capability probe or load-time symptom guard was added (`target/crates/gateway-s3/src/lib.rs:478`, `target/crates/gateway-s3/src/lib.rs:602`, `target/crates/server/tests/s3_multipart_routing.rs:760`). |
| T1 Structure | PASS | Routing stays inside the protocol adapter and preserves narrow backend seams; separate operation arms leave the later verb implementations local (`target/crates/gateway-s3/src/lib.rs:515`, `target/crates/gateway-s3/src/lib.rs:2256`). |
| T2 Shape | PASS | Explicit method/path/key combinations and one-pass duplicate detection make ambiguous requests refusals rather than first-wins dispatch; canonical decimal parsing preserves the stated range-check boundary (`target/crates/gateway-s3/src/lib.rs:580`, `target/crates/gateway-s3/src/lib.rs:610`, `target/crates/gateway-s3/src/lib.rs:636`). |
| T3 Runtime | PASS | Refused multipart requests reach no storage operation; wire exchanges have deadlines and fixture drop cancels its server, with the lifetime check passing (`target/crates/gateway-s3/src/lib.rs:2227`, `target/crates/server/tests/s3_multipart_routing.rs:126`, `target/crates/server/tests/s3_multipart_routing.rs:381`, `reviewer-evidence/green.log:79`). |
| T4 Contribution | PASS | Affected-path merged history and every closed/unmerged PR's file list were checked; docs currency is satisfied, and contribution-artifact auditing is N/A until its mandatory publish rerun (`reviewer-evidence/prior-art.log:1`, `target/docs/design/architecture/05-building-block-view.md:139`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Object preservation is checked after every refusal, bulk deletion has a live destructive control, and torn response framing is rejected; these oracles can expose the forbidden failure (`target/crates/server/tests/s3_multipart_routing.rs:189`, `target/crates/server/tests/s3_multipart_routing.rs:301`, `target/crates/server/tests/s3_multipart_routing.rs:555`, `target/crates/server/tests/s3_multipart_routing.rs:878`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this routing-only milestone for users — malformed shapes now return 400 while valid multipart operations remain 501, so the fix prevents data loss but does not provide multipart upload functionality (`target/docs/design/architecture/05-building-block-view.md:139`, `target/crates/server/tests/s3_multipart_routing.rs:643`, `target/crates/server/tests/s3_multipart_routing.rs:703`). |

No implementation defect remains in the reviewed scope. The real in-process server, SigV4 verifier, router, redb metadata, and filesystem chunk store exhibited the destructive failure before the fix and preserved the objects afterward. This is an exercised environment capable of the failure, not a fixture that excludes it. The brief's external dependency, `typos`, actually ran successfully (`reviewer-evidence/scanners.log:1`).

Independent verification used `cargo test --offline -p wyrd-server --test s3_multipart_routing` with only `crates/gateway-s3/src/lib.rs` stashed, leaving the new test unchanged. After stash pop, the routing, multipart-seam, bulk-delete, and listing integration suites passed 11, 7, 22, and 24 tests respectively (`reviewer-evidence/green.log:15`, `:43`, `:73`, `:90`). Gateway tests passed 97/97. `cargo clippy --offline -p wyrd-gateway-s3 -p wyrd-server --tests`, `typos`, docs lint, docs render/link audit, formatting, and diff whitespace checks passed (`reviewer-evidence/clippy.log:226`, `reviewer-evidence/scanners.log:1`, `reviewer-evidence/docs-render.log:3`). The target is the supplied self-contained base copy; reverse-application checking succeeded, and its tracked diff and new regression file were restored byte-for-byte.

The instance-root gate wrappers are absent from the supplied target by design. Their broader results below are adjudicated from the frozen logs, distinguished from the independent runs above.

| Gate evidence | Verdict | Basis |
|---------------|---------|-------|
| C4-ci | PASS | Full CI passed on attempt two; attempt one hit the known empty access-log capture, not a demonstrated routing defect (`gate-logs/C4-ci.log:2126`, `gate-logs/C4-ci.log:2792`, `gate-logs/C4-ci.log:6327`). |
| C4-verify | PASS | Frozen evidence agrees with the independent result: seven assertion failures before the fix, eleven passing tests with it (`gate-logs/C4-verify.log:12`, `gate-logs/C4-verify.log:130`). |
| C4-diff-cov | PASS | 392/392 instrumentable changed lines executed; 323 other changed lines were unscored, so this is not a claim of complete semantic coverage (`gate-logs/C4-diff-cov.log:169`). |
| C5-mutants | PASS | The captured run reports 58 mutants: 56 caught and two unviable, with no survivor reported (`gate-logs/C5-mutants.log:10`). |
| T4-batch-review | PASS | The captured three-pass review reports zero findings (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | PR/commit contribution artifacts are drafted after Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | The captured clippy commands compiled both the TiKV crate and server feature selection; they do not establish live-cluster behavior, which this routing brief does not require (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`). |

The earlier access-log caveat is settled for this patch by the existing [tracked flake #988](https://github.com/getwyrd/wyrd/issues/988). That issue records the identical empty capture on a change outside `crates/`; this review additionally ran ten complete baseline suites (90 tests each) and ten patched suites (97 each), all passing. The suggested tracing-cache cause remains unproven. Retain the flake under #988 rather than reopening it as #667 implementation work (`target/crates/gateway-s3/src/lib.rs:4919`, `reviewer-evidence/flake.log:1`, `reviewer-evidence/gateway-baseline-repeats.log:1`, `reviewer-evidence/gateway-patched-repeats.log:1`).

The earlier prior-art gap is also discharged. GitHub history at base `376b106` returned 40 commits for the gateway file, two for the existing seam test, 43 for the architecture document, and none for the new routing test. All 19 closed/unmerged PRs had their changed-file lists compared against all four affected paths; none overlapped. The searches were complete without another page. Relevant landed work and retrieval evidence are retained in `reviewer-evidence/prior-art.log:1` and `reviewer-evidence/prior-art.json:1`. The brief's previous-attempt dispositions were respected; the documented raw listing-parameter limitation is not reopened here.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue #667

I could not refute the fix for any form the brief lists. The red→green proof reproduces, and it runs the production path. I found one behaviour the patch makes newly destructive, and two gaps in what the patch's docs promise. All three are scope decisions for a human, so none is routed back to the builder.

Wire results below come from my own runs: a probe test (a throwaway test I added in scratch, not part of the patch) sent signed requests to both trees through `serve_s3`, with each tree in its own build directory.

## Needs a decision

- NEEDS-HUMAN [human] — `crates/gateway-s3/src/lib.rs:1982`: this patch turns a refused request into a live delete. `POST /b%2Fa?d%65lete` with a `<Delete>` body naming `k` answered `501` on base and left `/b/a/k` in place. On the fix it answers `200` and `/b/a/k` is gone (HEAD `404`). The bulk route never checks for a `/` in the decoded bucket, unlike the PUT arm (`lib.rs:2071`) and every multipart route. The plain `POST /b%2Fa?delete` already did this on base, so the caller gains no new power, and the brief's rule (6) covers only multipart routes. That makes it a scope call: either refuse the alias on the bulk route here, which changes the plain spelling too, or file an issue. It should not pass unnoticed, because the patch's own tests never send this request.
- NEEDS-HUMAN [human] — `docs/design/architecture/05-building-block-view.md:139`: the sentence "no spelling of `uploads`, `uploadId` or `partNumber` reaches a plain object verb" holds only for percent-encoded spellings. `DELETE /b/k?UPLOADID=U` answers `204` and deletes `k`. `PUT /b/k?uploadid=U&partnumber=1` answers `200` and overwrites `k`. Base behaves the same, and SigV4 signs these as different keys, so the code matches its stated "decoded once" rule. A human should decide whether the issue's "every spelling" includes letter case, or whether the doc should say "percent-encoded spelling". I could not check how real S3 treats these keys, so I don't claim it is a bug against S3.
- NEEDS-HUMAN [human] — `crates/gateway-s3/src/lib.rs:1997` (raw matching at `lib.rs:824`): the brief's invariant says an implemented form is reached the same way "whatever spelling the client uses" (`brief.md:75-76`), and listings still break that. `GET /b?pref%69x=nope` answers `200` and lists `k`, while `GET /b?prefix=nope` does not list it. `GET /b?max%2Dkeys=abc` answers `200`, while the plain form answers `400`. The patch documents this residual (`lib.rs:1949-1950`, `05-building-block-view.md:139`) and `brief.md:137` raises it, but neither the code nor the doc carries a `deferred: #N` issue number, so under the rubric it is not yet a settled deferral. The behaviour is on base too. If an issue already tracks it, ignore this bullet. The doc's example (`list%2Dtype=3`) also undersells it: a dropped `prefix` returns more keys than the client asked for.

## Gate claims that overstate the evidence

- `check-gates.json:53`: "11 test(s) ran red" is wrong as worded. The log shows `4 passed; 7 failed` (`gate-logs/C4-verify.log:130`), and my own red leg got the same 4 and 7. Those numbers fit the file's header, which says 7 legs fail on base and the rest are guards. The proof is still valid; only the gate's summary line overstates it. This is harness wording, so the builder has nothing to fix.
- `check-gates.json:37`: "xtask ci: all checks passed" hides a failed first attempt (`gate-logs/C4-ci.log:2078`, `crates/gateway-s3/src/lib.rs:4919`). This patch did not cause that failure. Over 500 runs per binary under equal load, `a_bodyless_response_is_recorded_complete_not_aborted` failed 6 times on base and 6 times on the fix. Base also had 1 failure in `the_access_line_carries_the_request_id_under_a_target_scoped_filter`. This answers the "unexplained" part of iteration 4's carry-forward (`brief.md:142`). The diff has no fix or marker for it, and I did not find the root cause, so it still needs its own issue.
- `gate-logs/T4-batch-review.log:10-12`: three "NO FINDINGS" passes in 38–61 s, each reporting 750–1,392 reasoning tokens on a 100 KB diff, are thin evidence. None of them raised the alias delete above.

## Attempted and could not refute

- **Red→green:** on base production with the kept test, 7 of 11 fail by assertion, including `DELETE /b/k?%75ploadId=…` answering `204` and deleting `k` in the gate's log. On the fix, 11 of 11 pass. The test drives the real `serve_s3`, SigV4 check and `Gateway`, with no mocks or copies of production code. After each row it checks every seeded object by HEAD and GET. Each row in the brief's legs (1) to (7) is present.
- **Decoding mismatch between routing and SigV4:** `percent_decode_utf8` (`lib.rs:3779`) and `sigv4::percent_decode` (`sigv4.rs:195`) use the same logic, including malformed escapes, and both convert invalid UTF-8 lossily. I found no input that signs as one key and routes as another. `upload%49d%3D=U` deletes `k`, but it signs as the different key `uploadId=`, so that is correct.
- **Path variants:** `PUT //b/k?part%4Eumber=1` and `DELETE //b/k?upload%49d=U`, which changed or deleted `k` on base, and `DELETE /b/k/?upload%49d=U`, which answered `204` on base, are all refused on the fix. `GET /b/?uploads`, lowercase `%2f` in the alias and `partNumber=%31` all classify correctly.
- **Part-number and shape edges:** `partNumber=1%20` answers `400`. `x-id` beside a multipart shape stays well-formed. An empty or bare `uploadId` routes to the Abort `501`, and checking the id is the seam's job.
- **Bulk delete:** `POST /b/?d%65lete&%61cl`, `POST /b?uploads&delete` and `POST /b%2Fa?delete&uploads` all answer `501` and leave `k` in place.
- **Test oracle:** the response parser refuses torn framing, and its own test proves it. `POST /b?d%2565lete` stays off the bulk route, so decoding happens only once.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified in this diff.

Reviewed the affected source at `$PDCA_TARGET`, including routing precedence, decoding, duplicate selectors, bucket aliases, bulk-delete refusals, and test framing and teardown. Validation used the frozen gate evidence; no builds or tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept this routing-only milestone for users — malformed shapes now return 400 while valid multipart operations remain 501, so the fix prevents data loss but does not provide multipart upload functionality (`target/docs/design/architecture/05-building-block-view.md:139`, `target/crates/server/tests/s3_multipart_routing.rs:643`, `target/crates/server/tests/s3_multipart_routing.rs:703`).
- [x] `crates/gateway-s3/src/lib.rs:1982`: this patch turns a refused request into a live delete. `POST /b%2Fa?d%65lete` with a `<Delete>` body naming `k` answered `501` on base and left `/b/a/k` in place. On the fix it answers `200` and `/b/a/k` is gone (HEAD `404`). The bulk route never checks for a `/` in the decoded bucket, unlike the PUT arm (`lib.rs:2071`) and every multipart route. The plain `POST /b%2Fa?delete` already did this on base, so the caller gains no new power, and the brief's rule (6) covers only multipart routes. That makes it a scope call: either refuse the alias on the bulk route here, which changes the plain spelling too, or file an issue. It should not pass unnoticed, because the patch's own tests never send this request.
- [x] `docs/design/architecture/05-building-block-view.md:139`: the sentence "no spelling of `uploads`, `uploadId` or `partNumber` reaches a plain object verb" holds only for percent-encoded spellings. `DELETE /b/k?UPLOADID=U` answers `204` and deletes `k`. `PUT /b/k?uploadid=U&partnumber=1` answers `200` and overwrites `k`. Base behaves the same, and SigV4 signs these as different keys, so the code matches its stated "decoded once" rule. A human should decide whether the issue's "every spelling" includes letter case, or whether the doc should say "percent-encoded spelling". I could not check how real S3 treats these keys, so I don't claim it is a bug against S3.
- [x] `crates/gateway-s3/src/lib.rs:1997` (raw matching at `lib.rs:824`): the brief's invariant says an implemented form is reached the same way "whatever spelling the client uses" (`brief.md:75-76`), and listings still break that. `GET /b?pref%69x=nope` answers `200` and lists `k`, while `GET /b?prefix=nope` does not list it. `GET /b?max%2Dkeys=abc` answers `200`, while the plain form answers `400`. The patch documents this residual (`lib.rs:1949-1950`, `05-building-block-view.md:139`) and `brief.md:137` raises it, but neither the code nor the doc carries a `deferred: #N` issue number, so under the rubric it is not yet a settled deferral. The behaviour is on base too. If an issue already tracks it, ignore this bullet. The doc's example (`list%2Dtype=3`) also undersells it: a dropped `prefix` returns more keys than the client asked for.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FLAKY — first run fail, confirm re-run pass (attempts: fail → pass); recorded pass (both runs in gate-logs/C4-ci.log). Confirm the red sample was environmental, not the patch — xtask ci: all checks passed
- [x] **The bulk-delete marker is still matched raw, which falls short of the brief's invariant.** See `crates/gateway-s3/src/lib.rs:1996` and `:720`. The brief says a form the gateway implements is reachable by exactly one route whatever the spelling, and it lists bulk delete among the routes that read the one classification. But `POST /b?d%65lete` with a valid `<Delete>` body signs as `delete`, yet answers `501 NotImplemented` instead of running DeleteObjects. The patch documents this at `lib.rs:718-719`, and `op_label` (`lib.rs:320`) depends on the same raw predicate. The request fails closed, so no data is lost and no SDK sends that spelling. The decision is one of two: (a) accept this and narrow the invariant's wording, or (b) decode the marker in both `dispatch` and `op_label` together. Option (b) widens a destructive path, so this is a judgment call, not a builder fix.
- [x] **UploadPartCopy will hit the UploadPart arm, and #669 is told to change "nothing else".** A `PUT /b/k?partNumber=1&uploadId=U` with an `x-amz-copy-source` header is classified `Multipart(UploadPart)` at `lib.rs:2059`. That happens before the CopyObject refusal at `lib.rs:2077`, which only the plain PUT arm reaches. Today the arm answers 501, so nothing breaks. But the doc comment at `lib.rs:2281` says #668–#670 each replace their arm "and nothing else", and `multipart_response` (`lib.rs:2282`) receives no headers, body or state. So #669 has to widen that signature anyway. If it does not also carry a copy-source refusal, an UploadPartCopy (empty body) would be stored as an empty part. That is the same data-loss shape `lib.rs:2065-2070` warns about. Either add this to #669's brief, or have the classifier refuse `x-amz-copy-source` on UploadPart now.
- [x] `crates/gateway-s3/src/lib.rs:1999` passes the raw query to `list_objects`, and `query_param` (`crates/gateway-s3/src/lib.rs:828`) still matches raw keys. An encoded listing parameter is therefore silently ignored. On the fixed tree, `GET /b?list-type=3` answers `400` but `GET /b?list%2Dtype=3` answers `200` with a V1 listing. `GET /b?pref%69x=zzz` lists every object where `prefix=zzz` lists none, `max%2Dkeys=0` is ignored, `list%2Dtype=2` returns the V1 shape, and `encoding%2Dtype=bogus` skips its `400`. The base tree does the same, so the patch did not cause this. But it breaks the invariant at `brief.md:75-76` ("whatever spelling the client uses"), which is the argument round 2 used to pull `d%65lete` into this patch. Decide whether listing parameters belong here or in a tracked issue; under the rubric's out-of-scope rule, I would file an issue.
- [x] `crates/gateway-s3/src/lib.rs:1984` lets a bucket alias reach bulk delete under a new spelling. `POST /b%2Fa?d%65lete` with a body naming `k` answered `501` on the base. On the fixed tree it answers `200`, deletes `b/a/k`, and reports the deleted key as `k`. The plain `POST /b%2Fa?delete` already did this on the base, so the patch adds a second spelling, not a new power. The patch refuses this alias on the multipart routes (`crates/gateway-s3/src/lib.rs:2229`) and the PUT arm keeps its refusal (`crates/gateway-s3/src/lib.rs:2073`), but the bulk route does not use it. No test covers the alias on bulk delete. `brief.md:48-54` limits the alias rule to multipart shapes, so extending it is a scope decision. Refusing it would change the plain spelling's behaviour too.
- [x] `check-gates.json:105-108` records T4 as a pass with "0 blocking, 0 recorded-rejected, 0 noise-dropped". The oracle is described as three codex passes, but `gate-logs/T4-batch-review.log:6` shows 18.43 seconds for a diff of about 98 KB, and the log has no per-pass output. Earlier rounds of this bundle found 4 and then 2 blocking items. The result may be correct or served from a cache, but the log does not show that three passes reviewed this diff. I was not given `review-batch.md`, so I could not check. T4 gates acceptance, so someone should open that file and confirm it.
- [x] `crates/gateway-s3/src/lib.rs:1982`: the patch opens one more spelling of a bucket-alias bulk delete. A signed `POST /b%2Fa?d%65lete` with `<Key>k</Key>` answered `200` and deleted object `b/a/k`. The original code refused that spelling with `501`. The plain spelling `POST /b%2Fa?delete` deletes `b/a/k` on both trees, so the defect is older than this patch and any client can already reach it. Brief item (6) only covers multipart routes, and the brief never asked for the encoded marker to reach bulk delete (that came from the iteration 2 carry-forward). **Decision needed:** either add the `invalid_bucket_name` check to this route, which is one line now that `bucket_name` is in scope, or decline it here with a tracking issue, as the rubric's out-of-scope rule says. This does not block the fix.
- [x] `gate-logs/T4-batch-review.log:10`, `check-gates.json:111`: I can't confirm the T4 pass from its evidence. This is the gate that failed in rounds 1 and 2. It is described as three codex review passes, but it finished in 11.02 seconds on a 100 KB patch and its log holds only the `0 blocking` summary line. That could be a cached result or a review that never really ran; I can't tell which, and `review-batch.md` is not in my inputs. Please open that file before relying on this pass. This says nothing against the code.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
