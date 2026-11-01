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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 371 of 371 instrumentable changed lines executed (floor 80%); 371 of 684 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 50 mutants tested in 6m: 48 caught, 2 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.08s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #667: prevent encoded multipart selectors from overwriting or deleting objects; independent red→green verification passes, with documentation scope and release fitness left for human decision.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The refusal-only contract gives falsifiable status, error-code and object-survival criteria for all seven request families; `brief.md:20`, `target/crates/server/tests/s3_multipart_routing.rs:276`. |
| C2 Reproduction (red pre-fix) | PASS | Independently observed encoded PUT return 200 and overwrite k, and encoded DELETE return 204 and remove k; nine tests ran, five failed by assertion; `reviewer-evidence/red-green.log:295`, `reviewer-evidence/red-green.log:362`. |
| C3 Change | PASS | The two-file change stays within routing and regression coverage, without serving multipart verbs or expanding backend/persistence scope; `patch.diff:1`, `patch.diff:916`, `target/crates/gateway-s3/src/lib.rs:2260`. |
| C4 Verification (red→green) | PASS | Restoring the production patch changed the same suite from five assertion failures to 9/9 passing; 97 gateway tests and fmt/typos/docs lint also passed independently; `reviewer-evidence/red-green.log:397`, `reviewer-evidence/checks.log:1`, `reviewer-evidence/checks.log:135`. |
| C5 Causal adequacy | PASS | Routing now agrees with signed key identity, closing the demonstrated destructive fall-through; exact byte-survival checks and a working bulk-delete control make refusal evidence non-vacuous; `target/crates/gateway-s3/src/lib.rs:479`, `target/crates/gateway-s3/src/lib.rs:781`, `target/crates/server/tests/s3_multipart_routing.rs:192`, `target/crates/server/tests/s3_multipart_routing.rs:878`. |
| T1 Structure | PASS | Classification stays inside the wire adapter and leaves the narrow gateway seams intact; subsequent verb implementations have separate operation arms; `target/crates/gateway-s3/src/lib.rs:517`, `target/crates/gateway-s3/src/lib.rs:2260`. |
| T2 Shape | PASS | The 84,989-byte patch remains below the 100 KB backstop; fixed selector slots enforce duplicate rejection and strict decimal grammar without new dependencies; `brief.md:114`, `target/crates/gateway-s3/src/lib.rs:581`, `target/crates/gateway-s3/src/lib.rs:611`. |
| T3 Runtime | PASS | Refused forms cannot reach object writes, and test exchanges have deadlines with server cancellation on fixture drop; no new destructive or concurrent production path needs DST coverage; `target/crates/gateway-s3/src/lib.rs:2035`, `target/crates/server/tests/s3_multipart_routing.rs:129`, `target/crates/server/tests/s3_multipart_routing.rs:384`. |
| T4 Contribution | NEEDS-HUMAN | Resolve #667's explicit docs exclusion against the docs-currency requirement—architecture still promises 501 for every multipart form, and the batch gate remains red for that mismatch; `brief.md:94`, `target/AGENTS.md:169`, `target/docs/design/architecture/05-building-block-view.md:137`, `gate-logs/T4-batch-review.log:10`. |
| T5 Judgment | PASS | The previous torn-framing and detached-server defects now have passing executable regression checks; path-based prior-art review found no rejected work touching either file; `target/crates/server/tests/s3_multipart_routing.rs:533`, `target/crates/server/tests/s3_multipart_routing.rs:597`, `reviewer-evidence/prior-art.log:72`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the refusal-only release boundary—malformed shapes return 400 and well-formed operations remain 501 until #668–#670; the demonstrated data-loss fix does not provide usable multipart uploads; `brief.md:3`, `target/crates/gateway-s3/src/lib.rs:2246`, `target/crates/gateway-s3/src/lib.rs:2260`. |

No additional implementation defect was found. The independent run establishes the data-loss fix, the prior iteration's two implementation findings are resolved, and the remaining contribution issue is a scope decision rather than an instruction to change the agreed wire behavior.

The red leg stashed only the changed gateway file, retaining the new integration test. All nine tests compiled and ran: four passed and five failed by assertion. The encoded PUT both returned 200 and changed the stored bytes; the encoded DELETE returned 204 and left HEAD/GET at 404. After `git stash pop`, all nine passed, including object preservation, the destructive bulk-delete control, incomplete-response rejection and fixture cancellation. Both changed files' hashes were restored and match the patch's destination blobs; reverse-application checking succeeds, so there is no stale-target caveat (`reviewer-evidence/red-green.log:295`, `reviewer-evidence/red-green.log:362`, `reviewer-evidence/red-green.log:400`, `reviewer-evidence/grounding.log:5`). The denylist-precedence test is also red on the base because it names the multipart key instead of tagging/acl; the brief's description of that case as green on both trees is inaccurate, but its required final behavior is met (`gate-logs/C4-verify.log:32`).

The frozen gates support the following bounded conclusions; the instance-scoped wrappers are absent from this disposable target, and their full logs were read rather than treating that absence as a defect (`reviewer-evidence/grounding.log:15`).

- **C4-ci: PASS from frozen evidence.** The log runs typos, docs checks, fmt, clippy/build/tests, dependency checks and DST, and ends with all checks passed (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:3860`, `gate-logs/C4-ci.log:4513`). Independent reruns additionally passed fmt, the actual `typos` binary, docs lint and 97 gateway tests (`reviewer-evidence/checks.log:3`, `reviewer-evidence/checks.log:135`). The brief's external dependency `typos` was exercised without a substitute.
- **C4-verify: PASS independently reproduced. C4-diff-cov: PASS from frozen evidence.** Coverage is 371/371 instrumentable changed lines, with 313 other changed lines unscored; it is not a claim of measured coverage for every changed line (`gate-logs/C4-diff-cov.log:224`).
- **C5-mutants: PASS from frozen evidence.** The log reports 48 caught and two unviable out of 50, with no survivors; it does not supply individual mutant details (`gate-logs/C5-mutants.log:10`). The causal review independently found a correction to decoded routing, with no capability probe or fallback masking a load-time cause (`target/crates/gateway-s3/src/lib.rs:479`, `target/crates/gateway-s3/src/lib.rs:603`).
- **host-tikv: PASS from frozen evidence.** Both requested feature compilations finish successfully; this establishes compilation, not a live TiKV exercise (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`). This routing slice does not require a live TiKV service.
- **T4-batch-review: FAIL as recorded, one distinct documentation issue.** Its two reports describe the same contradiction, grounded in the living architecture paragraph. The brief explicitly excludes docs, while the standing rubric requires documentation for altered API behavior. Prefer a narrowly authorized correction of that paragraph; otherwise record the explicit scope/rubric disposition. This review does not clear the recorded gate failure or route the scope conflict back as an implementation defect (`gate-logs/T4-batch-review.log:10`, `brief.md:94`, `target/AGENTS.md:169`, `target/docs/design/architecture/05-building-block-view.md:137`).
- **T4-contribution: N/A.** The contribution artifacts are intentionally absent at Check; their substantive audit must run at publish. No human escalation is owed for that deferred row (`gate-logs/T4-contribution.log:10`).

The prior-art check ran against both exact affected paths. GitHub history at the frozen base returned 40 gateway-file commits and no history for the new test; relevant predecessors include the original subresource refusal, decoded bucket routing, bulk-delete exclusion and multipart seam. Closed-PR searches were supplemented by enumerating all 19 closed, unmerged PRs and checking each complete changed-file list: none touched either affected path. Queries, commit identities and file inventories are preserved in `reviewer-evidence/prior-art.log:1` and `reviewer-evidence/prior-art.log:71`. Source findings remain grounded solely in the supplied target; no other checkout or builder notes were consulted.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue #667 (multipart routing classifier)

**Result:** the data-loss fix held against every attack. Two defects remain: the required docs update is missing, and an encoded `delete` key breaks the brief's invariant. Neither one loses data.

## Findings

- NEEDS-HUMAN [impl] — `docs/design/architecture/05-building-block-view.md:137` still says "the S3 wire still refuses every multipart form with `501`". That is now false: `PUT /b/k?partNumber=1` answers `400 InvalidArgument` (`crates/gateway-s3/src/lib.rs:2035`, table row in `crates/server/tests/s3_multipart_routing.rs`, leg 2). The diff touches no docs, and this is the same gap behind the red T4 gate (`gate-logs/T4-batch-review.log:10`). The unwarranted claim is in `brief.md:94`, "docs (no API operation is added)". The AGENTS.md Docs currency rule also covers a change that *alters* an API operation, and calls it a merge requirement. Fix: correct that sentence in the same PR, stating that well-formed forms answer `501` and ill-formed shapes answer `400`.

- NEEDS-HUMAN [impl] — `crates/gateway-s3/src/lib.rs:1972` still detects bulk delete on the raw query (`is_delete_subresource`, `:696`), so one spelling of an implemented form does not reach its route. Probe over the wire: `POST /b?delete` runs the bulk handler, but `POST /b?d%65lete`, which SigV4 signs as the same key, answers `501` naming `delete`. This contradicts two things:
  - the brief's invariant, "a form it does implement is reachable by exactly one route, whatever spelling the client uses" (`brief.md:75-76`), and its scope line naming bulk delete as a reader of the classification (`brief.md:82-83`);
  - the new comment at `lib.rs:1952-1954`, "no spelling of a key reaches a route its plain spelling cannot".

  It fails closed and the base behaves the same, so this is not data loss. The `lib.rs:694` comment records the residual, but no test pins it. Fix: match on `DecodedQuery` here and in `op_label` (`:320`), which must keep mirroring this route, and add a wire row. Otherwise, narrow the comment and record the exception.

## Notes (no action routed)

- `check-gates.json:48` says "9 test(s) ran red", which overstates the result. Only 5 of the 9 failed (`gate-logs/C4-verify.log:107`), and I got the same 5 failed, 4 passed. The red itself is real. On the base, `PUT ?part%4Eumber=1` overwrites `k` and `DELETE ?upload%49d=…` deletes it. Leg 5 is red on the base only because of message wording, where `brief.md:69` predicted green on both trees. The test file header records this.
- `crates/server/tests/multipart_gateway_seam.rs:77-78` is now stale. It cites the removed `unsupported_subresource` and says the 501 names the "FIRST denylisted query key", but these keys have left the denylist. The test still passes because the new message keeps the wording "the `{key}` S3 subresource". This is comment-only and outside the brief's file scope, so #668 can correct it when it changes these routes.
- These different keys still reach the plain verb, and each row overwrote or deleted `k`:
  - `DELETE /b/k?uploadid=U` → `204`
  - `PUT /b/k?partnumber=1&uploadid=U` → `200`
  - `DELETE /b/k?uploadId%20=U`, `?uploadId%00=U` and `?uploadId%FF=U` → `204`
  - `DELETE /b/k?x=1;uploadId=U` → `204`

  I did not file these as refutations. SigV4 signs each one as a key other than `uploadId` or `partNumber`, and the denylist comment (`lib.rs:403-409`) says it is not a ban on all query strings. A human who reads "every spelling" as including case changes would need to widen the brief.

## What I tried and could not break

- **The evidence:** with toolchain 1.96.0, I rebuilt in scratch and got green with the fix (9 of 9). With `lib.rs` reverted to the base, I got 5 failed by assertion and 4 passed. Forty repeat runs had no failures. The test drives the production `serve_s3` over TCP with hand-signed requests, so it does not use a parallel copy of the routing. The harness checks framing strictly, and every row re-checks HEAD, ETag and GET on each seeded object.
- **Percent-encoding:** lowercase hex, a fully encoded key, `upload%49%64`, and double encoding (`%254E` stays a different key) all classify as intended. `sigv4.rs:195` and `lib.rs:3783` use the same decoding rules, so routing and signing agree on what a key is, including the lossy UTF-8 case.
- **`partNumber` values:** `+1`, `%2B1`, `01`, `1%00`, Arabic digit `%D9%A3`, a 20-digit number and `4294967296` all answer `400`. `%31` is accepted as `1`, which is correct after one decode.
- **Paths:** `//b/k`, `/b//k`, `/b/k/` and `/b/` never reached a plain verb or a listing, and `/?uploads` answers `400 InvalidRequest`.
- **Other shapes:**
  - An SDK's `x-id` beside a shape, `uploads=`, and a trailing or doubled `&` still route to the operation.
  - An empty `uploadId` routes to the operation and answers `501`. The brief leaves checking the id to the verb.
  - Repeats across spellings are refused.
  - `HEAD`, `OPTIONS` and `PATCH` with a multipart key answer `400`.
  - A method named `A&B` comes back XML-escaped in the message.
- **Denylist and bulk delete:** `DELETE ?version%49d=v` and `PUT ?%61cl` now answer `501`. `POST /b?delete&part%4Eumber=1` is refused and leaves the objects alone.
- **Gates:** the mutation gate shows 48 caught and none missed (`gate-logs/C5-mutants.log:13`). The largest spans the diff-coverage gate left unscored, the ones I read at `lib.rs:554-580` and `:589-602`, are doc comments and declarations.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No new findings on either requested lens: introduced correctness bugs, or actionable reuse, simplification, and efficiency issues.

The earlier findings are addressed: chunked responses require complete terminal framing (`crates/server/tests/s3_multipart_routing.rs:506`), the fixture retains and aborts its server task (`crates/server/tests/s3_multipart_routing.rs:129`), and multipart parameters use fixed slots populated in one pass (`crates/gateway-s3/src/lib.rs:611`).

Validation used the frozen gate evidence: CI passed; all 9 integration tests passed with the patch, with 5 assertion failures on the reverted production code; mutation testing caught 48 mutants with 2 unviable. The docs-currency finding already recorded by T4 is unchanged; this advisory does not override that gate.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Resolve #667's explicit docs exclusion against the docs-currency requirement—architecture still promises 501 for every multipart form, and the batch gate remains red for that mismatch; `brief.md:94`, `target/AGENTS.md:169`, `target/docs/design/architecture/05-building-block-view.md:137`, `gate-logs/T4-batch-review.log:10`.
- [ ] Validation — fitness-to-purpose — Accept the refusal-only release boundary—malformed shapes return 400 and well-formed operations remain 501 until #668–#670; the demonstrated data-loss fix does not provide usable multipart uploads; `brief.md:3`, `target/crates/gateway-s3/src/lib.rs:2246`, `target/crates/gateway-s3/src/lib.rs:2260`.
- [ ] `docs/design/architecture/05-building-block-view.md:137` still says "the S3 wire still refuses every multipart form with `501`". That is now false: `PUT /b/k?partNumber=1` answers `400 InvalidArgument` (`crates/gateway-s3/src/lib.rs:2035`, table row in `crates/server/tests/s3_multipart_routing.rs`, leg 2). The diff touches no docs, and this is the same gap behind the red T4 gate (`gate-logs/T4-batch-review.log:10`). The unwarranted claim is in `brief.md:94`, "docs (no API operation is added)". The AGENTS.md Docs currency rule also covers a change that *alters* an API operation, and calls it a merge requirement. Fix: correct that sentence in the same PR, stating that well-formed forms answer `501` and ill-formed shapes answer `400`.
- [ ] `crates/gateway-s3/src/lib.rs:1972` still detects bulk delete on the raw query (`is_delete_subresource`, `:696`), so one spelling of an implemented form does not reach its route. Probe over the wire: `POST /b?delete` runs the bulk handler, but `POST /b?d%65lete`, which SigV4 signs as the same key, answers `501` naming `delete`. This contradicts two things:
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b
- [ ] **The bulk-delete marker is still matched raw, which falls short of the brief's invariant.** See `crates/gateway-s3/src/lib.rs:1996` and `:720`. The brief says a form the gateway implements is reachable by exactly one route whatever the spelling, and it lists bulk delete among the routes that read the one classification. But `POST /b?d%65lete` with a valid `<Delete>` body signs as `delete`, yet answers `501 NotImplemented` instead of running DeleteObjects. The patch documents this at `lib.rs:718-719`, and `op_label` (`lib.rs:320`) depends on the same raw predicate. The request fails closed, so no data is lost and no SDK sends that spelling. The decision is one of two: (a) accept this and narrow the invariant's wording, or (b) decode the marker in both `dispatch` and `op_label` together. Option (b) widens a destructive path, so this is a judgment call, not a builder fix.
- [ ] **UploadPartCopy will hit the UploadPart arm, and #669 is told to change "nothing else".** A `PUT /b/k?partNumber=1&uploadId=U` with an `x-amz-copy-source` header is classified `Multipart(UploadPart)` at `lib.rs:2059`. That happens before the CopyObject refusal at `lib.rs:2077`, which only the plain PUT arm reaches. Today the arm answers 501, so nothing breaks. But the doc comment at `lib.rs:2281` says #668–#670 each replace their arm "and nothing else", and `multipart_response` (`lib.rs:2282`) receives no headers, body or state. So #669 has to widen that signature anyway. If it does not also carry a copy-source refusal, an UploadPartCopy (empty body) would be stored as an empty part. That is the same data-loss shape `lib.rs:2065-2070` warns about. Either add this to #669's brief, or have the classifier refuse `x-amz-copy-source` on UploadPart now.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Resolve #667's explicit docs exclusion against the docs-currency requirement—architecture still promises 501 for every multipart form, and the batch gate remains red for that mismatch; `brief.md:94`, `target/AGENTS.md:169`, `target/docs/design/architecture/05-building-block-view.md:137`, `gate-logs/T4-batch-review.log:10`.; `docs/design/architecture/05-building-block-view.md:137` still says "the S3 wire still refuses every multipart form with `501`". That is now false: `PUT /b/k?partNumber=1` answers `400 InvalidArgument` (`crates/gateway-s3/src/lib.rs:2035`, table row in `crates/server/tests/s3_multipart_routing.rs`, leg 2). The diff touches no docs, and this is the same gap behind the red T4 gate (`gate-logs/T4-batch-review.log:10`). The unwarranted claim is in `brief.md:94`, "docs (no API operation is added)". The AGENTS.md Docs currency rule also covers a change that *alters* an API operation, and calls it a merge requirement. Fix: correct that sentence in the same PR, stating that well-formed forms answer `501` and ill-formed shapes answer `400`.; `crates/gateway-s3/src/lib.rs:1972` still detects bulk delete on the raw query (`is_delete_subresource`, `:696`), so one spelling of an implemented form does not reach its route. Probe over the wire: `POST /b?delete` runs the bulk handler, but `POST /b?d%65lete`, which SigV4 signs as the same key, answers `501` naming `delete`. This contradicts two things:; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_667/review-b
- By / date: auto-iterate / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
