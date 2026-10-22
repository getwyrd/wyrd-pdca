# Result — issue 620 / s3-object-key-length-limit

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The S3 gateway enforces S3's 1,024-byte object-key limit nowhere. A `PUT
  /{bucket}/{key}` stores a key of any length: the decoded key is composed at
  `crates/gateway-s3/src/lib.rs:1765-1769` and streamed straight into the backend write. The
  bulk-delete body cap's doc comment (`MAX_DELETE_BODY_BYTES`, `:489-507`) sizes its floor on
  "1000 keys × a 1024-byte key" while `parse_delete_request` (`:2171-2245`) accepts any non-empty
  `<Key>` (`:2208-2211`) up to the whole 8 MiB cap, and nothing says which of the two is meant
  (the #612 review follow-up in this issue's thread). The limit must not land as a refusal in
  bulk delete: over-limit objects can already exist (stored before this fix, or written by the
  data-plane `wyrd put --key`, `crates/server/src/cli.rs:567`), and some of them can only be
  removed by bulk delete (see criterion 3).
- Success criterion: A NEW wire-level test over the in-process gateway (signed raw HTTP and/or
  the stock SDK, as the peer suites do) shows:
  (1) **Creation refuses an over-limit key.** A PUT whose decoded key is 1,025 UTF-8 bytes answers
  `400` with S3 error code `KeyTooLongError`, before the body is stored: afterwards HEAD of that
  key is `404` and a listing of the bucket does not show it.
  (2) **The limit counts UTF-8 bytes of the decoded key, not characters or wire bytes.** A key of
  512 × `é` (1,024 bytes, 512 characters) is accepted and round-trips (HEAD `200`). A key of
  342 × `€` (1,026 bytes, 342 characters) is refused `KeyTooLongError`. A 1,024-byte key sent
  percent-encoded on the wire (so its request path is far longer than 1,024) is accepted. The
  limit applies to the key alone, not to `"{bucket}/{key}"`.
  (3) **Bulk delete adds no key-length refusal: a key the existing request limits can carry is
  deleted, however long.** (Those limits are unchanged: the 8 MiB body cap and the request-target
  ceiling below. This leg does not claim to reach a key too large for the body cap; see the
  Invariant.) Seed `b/<'a' × 1025>` and
  `b/<'c' × 70000>` through `Gateway::put_object` on the `Arc<Gateway>` clone (as in (4)), and PUT
  `b/ok` over the wire. One signed `DeleteObjects` naming all three keys answers `200` with a
  `<Deleted>` row for each key and **no** `<Error>` row. Afterwards all three are gone: HEAD of
  `b/ok` and of `b/<'a' × 1025>` is `404`, and the 70,000-byte key is absent when read through the
  `Arc<Gateway>` clone (`Gateway::get_object`, `crates/server/src/lib.rs:298`, returns `None`, as `crates/server/tests/e2e.rs:40` reads it).
  Its HEAD cannot show this, because hyper answers `414` for a request target over 65,534 bytes
  before the gateway runs (`http` 1.4.2, `Uri` `MAX_LEN = u16::MAX - 1`). That `414` is why bulk
  delete is the only S3 way to remove such a key, and why it must not refuse it.
  (4) **Single-object DELETE still removes an over-limit object.** Seed `b/<'a' × 1025>` through
  the existing backend API, `Gateway::put_object` (`crates/server/src/lib.rs:235`), on an
  `Arc<Gateway>` clone the test keeps before handing the gateway to `S3Gateway::new`
  (`crates/gateway-s3/src/lib.rs:232` takes `Arc<G>`). Then: signed HEAD of that key is `200`;
  signed `DELETE` is `204`; HEAD again is `404`. The HEAD-before and HEAD-after legs are required,
  because DELETE answers `204` for an absent key too (`crates/gateway-s3/src/lib.rs:1887-1890`),
  so `204` alone proves nothing.
  (5) **No bucket-separator alias around the limit.** The gateway splits the path on the first
  literal `/` before decoding (`crates/gateway-s3/src/lib.rs:567-573`) and joins the decoded
  bucket and key into one flat key (`:1765-1769`). So `PUT /b%2Fa/<'x' × 1023>` has a 1,023-byte
  request key yet stores the same object as `/b/a/<'x' × 1023>`, whose key under bucket `b` is
  1,025 bytes. A PUT whose decoded bucket segment contains `/` must be refused `400`
  `InvalidBucketName` (S3 bucket names cannot contain `/`) before the body is stored: afterwards
  HEAD of `/b/a/<'x' × 1023>` is `404`. Send this one as signed raw HTTP (the SDK will not put
  `%2F` in a bucket name).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: S3 key-length enforcement where an object is created (PUT); the refusal of a PUT
  whose decoded bucket segment contains `/` (criterion 5); the `MAX_DELETE_BODY_BYTES` doc
  comment (`crates/gateway-s3/src/lib.rs:489-507`) made true about what bounds one bulk-delete
  entry (the body cap alone, deliberately: no length refusal is added, so any key the cap can
  carry stays deletable; the comment must not claim more than that); and the
  architecture doc's account of the limit (`docs/design/architecture/05-building-block-view.md`,
  next to the L1 access-layer table at `:126-135`): where it applies, that bulk delete has no
  per-key bound and why, and the residual limit (a key too large for the body cap, writable only
  through the in-process API, has no S3 exit) / out of scope: recovering a key too large for the
  8 MiB body cap (a pre-existing gap this fix does not widen; see Invariant); any key-length bound in bulk delete, whole-request or
  per-key `<Error>` row (rejected at the iteration-4 sign-off); CopyObject, which answers `501`
  today (`crates/gateway-s3/src/lib.rs:1800-1806`; when #504 implements it, the same rule applies
  to its destination key); single-object GET, HEAD and DELETE (unchanged, see criterion 4);
  listing parameters (`prefix`, `start-after`, `continuation-token`); `wyrd put --key` and any
  native key limit below the S3 gateway (filed as #893); bucket naming rules except the one
  criterion (5) needs (no `/` in a decoded bucket, on PUT only; GET, HEAD and DELETE on such a
  path stay as they are, since they create nothing); the multipart verbs (#508's slices apply the
  same rule to CreateMultipartUpload's key when they land); the 8 MiB cap value itself; the `414`
  hyper returns for request targets over 65,534 bytes.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 92.9% — 143 of 154 instrumentable changed lines executed (floor 80%); 154 of 276 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 5 mutants tested in 26s: 5 caught

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_620/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.84s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect was found in #620’s fix to enforce S3’s 1,024-byte decoded-key limit on PUT while preserving deletion of existing over-limit keys.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The five observable criteria distinguish creation admission from legacy deletion and explicitly preserve existing request limits; `brief.md:24`, `brief.md:74`. |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes produces three assertion failures (200 instead of 400), while both deletion controls pass; `reviewer-red.log:7`, `reviewer-red.log:43`. |
| C3 Change | PASS | The change stays within the approved creation boundary, regression coverage, and living documentation; deletion policy remains unchanged; `crates/gateway-s3/src/lib.rs:1817`, `docs/design/architecture/05-building-block-view.md:139`. |
| C4 Verification (red→green) | PASS | Restoring the patch yields 5/5 passing wire tests; CI’s two cache-permission failures pass on policy-identical retries with writable cache storage; `reviewer-green-restored.log:13`, `reviewer-ci.log:4033`, `reviewer-deny-writable.log:1`, `reviewer-deny-all-writable.log:1`. |
| C5 Causal adequacy | PASS | The missing admission boundary and bucket-separator alias are addressed before streaming; rejected writes never reach the backend, and there is no capability-probe/load-time workaround; `crates/gateway-s3/src/lib.rs:1851`, `crates/gateway-s3/src/lib.rs:1862`, `crates/gateway-s3/src/lib.rs:4770`. |
| T1 Structure | PASS | Protocol-specific admission remains in the S3 adapter without widening backend traits or changing persisted data; `crates/gateway-s3/src/lib.rs:1798`, `crates/gateway-s3/src/lib.rs:1817`. |
| T2 Shape | PASS | Existing decoding, error-response, and signing helpers preserve local conventions; the changed API behavior is documented and formatting/spelling checks pass; `crates/gateway-s3/src/lib.rs:1852`, `crates/server/tests/s3_key_length_limit.rs:56`, `docs/design/architecture/05-building-block-view.md:139`, `reviewer-ci.log:2`, `reviewer-ci.log:21`. |
| T3 Runtime | PASS | Real backend reads establish deletion rather than trusting idempotent success responses, and the harness bounds exchanges and aborts its server on drop; `crates/server/tests/s3_key_length_limit.rs:83`, `crates/server/tests/s3_key_length_limit.rs:177`, `crates/server/tests/s3_key_length_limit.rs:334`, `crates/server/tests/s3_key_length_limit.rs:381`. |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit must rerun at publish, so their current absence requires no human clearance; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | Exact-path prior-art checks cover merged history and all 396 closed PRs, with no closed-unmerged match; the patch respects the settled rejection of bulk-delete length restrictions; `reviewer-prior-art.log:49`, `reviewer-prior-art.log:51`, `brief.md:155`, `crates/server/tests/s3_key_length_limit.rs:325`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that the creation-only S3 compatibility boundary satisfies #620’s user needs — automated checks establish the specified wire behavior, while recovery beyond the unchanged bulk-body cap remains outside this repair’s scope; `brief.md:123`, `docs/design/architecture/05-building-block-view.md:139`. |

No builder-fixable defect was found. The remaining human decision is fitness-to-purpose; the contribution audit is deferred to publish by design.

All source citations above resolve in `$PDCA_TARGET`, the supplied disposable target based on `8ebd1c02b3da79db86a89fa54fdea8a838f0448a`. Input and evidence citations resolve in this report’s directory. The patch reverse-application check succeeds after restoration, the temporary stash is gone, and the target retains exactly its original two modified files and added test.

Independent execution supports the behavior directly. `cargo test --locked --offline -p wyrd-server --test s3_key_length_limit` passed with the patch, failed after stashing the production/doc changes while retaining the new test, and passed after `git stash pop`. The red failures specifically cover the 1,025-byte key, the 1,026-byte multibyte key, and the encoded bucket separator. Both legacy-deletion tests pass on either side (`reviewer-red.log:7`; `reviewer-green-restored.log:7`). The green tests also verify exact-limit acceptance, encoded-wire independence, an absent refused object plus a visible listing control, and actual removal of the 70,000-byte fixture (`crates/server/tests/s3_key_length_limit.rs:273`, `crates/server/tests/s3_key_length_limit.rs:288`, `crates/server/tests/s3_key_length_limit.rs:381`).

The independent CI run passed formatting, clippy, workspace build/tests, docs lint/render, spelling, dependency hygiene, conformance, structural scanners, and the seeded simulation suite. Its aggregate exit was 1 solely because both advisory invocations attempted an exclusive lock under the read-only home cache (`reviewer-ci.log:3405`, `reviewer-ci.log:3410`, `reviewer-ci.log:4033`). Both then passed using the real `cargo deny` with scratch copies of the original policies differing only in `advisories.db-path`; parsed-policy equality was checked after removing that storage setting (`reviewer-deny-writable.log:1`; `reviewer-deny-all-writable.log:1`). This resolves the host caveat without changing advisory policy. The three external dependencies named in `brief.md:138` were exercised.

The remaining frozen evidence supports its stated, limited claims:

- C4-ci records all checks passing, including both advisory walls and the docs renderer (`gate-logs/C4-ci.log:16`, `gate-logs/C4-ci.log:3610`, `gate-logs/C4-ci.log:4237`).
- C4-verify agrees with the independent result: three failures among five tests before the fix, all five passing afterward (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:53`).
- Diff coverage reports 143/154 instrumentable changed lines, 92.9%, with 122 unscored lines; its denominator includes co-located test code, so this is not a claim of exhaustive production-path coverage (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:193`).
- The mutation log reports five tested and five caught; it does not enumerate the mutations (`gate-logs/C5-mutants.log:10`).
- The batch-review log reports zero blocking findings, and the TiKV log shows both feature-enabled clippy invocations finishing (`gate-logs/T4-batch-review.log:10`, `gate-logs/host-tikv.log:111`, `gate-logs/host-tikv.log:209`). These instance/host-scoped checks were assessed from their captured logs.

Prior art was checked by each affected file path, including merged commit history and the complete returned closed-PR inventory. Forty-six merged PRs match; none of the nineteen closed-unmerged PRs touches these paths. Relevant precedents include bulk deletion in #612 and the adjacent PUT-size admission in #899 (`reviewer-prior-art.log:5`, `reviewer-prior-art.log:25`, `reviewer-prior-art.log:49`). The brief’s recorded rejection of bulk-delete length restrictions is respected; its settled native-limit deferral is not reopened.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 620 (S3 object-key length limit)

**Verdict: I could not break the fix.** I re-ran the red→green proof myself and tried nine
alternate spellings of an over-limit PUT. All of them hit the new guard. I found one gap, in
how strong the test is: leg 3 does not catch a bulk-delete length bound set anywhere above
70,000 bytes.

## Findings

- NEEDS-HUMAN [impl] — **Leg 3 only protects keys up to 70,000 bytes.** The brief's
  Invariant and the patch's own doc (`crates/gateway-s3/src/lib.rs:498-500`, `:518-525`)
  promise that bulk delete has **no** length bound below the 8 MiB body cap. The tests only
  check that promise up to the 70,000-byte fixture (`crates/server/tests/s3_key_length_limit.rs:328`,
  `crates/gateway-s3/src/lib.rs:4787`). Concrete failing case: in a scratch copy I added
  `if key.len() > 100_000 { return Err(DeleteRequestError::Malformed); }` right after the
  empty-key check in `parse_delete_request` (`crates/gateway-s3/src/lib.rs:2331-2333`).
  That mutant passed every test: `s3_key_length_limit` (5/5), all 82 `wyrd-gateway-s3` lib
  tests, and `s3_delete_objects` (22/22). Yet it strands every key of 100,001 to 131,072 bytes
  that `wyrd put --key` can write. Those keys are past the `414` limit, so bulk delete is
  their only S3 exit, which is exactly the iteration-4 sign-off defect. The fix is cheap. Over
  the real wire, bulk delete of an 8,388,000-byte key seeded through `Gateway::put_object`
  works today (200, `<Deleted>`, no `<Error>`, gone afterwards), and three such deletes
  (131 KB, 3 MB, 8.4 MB) took about 1 s in total. So leg 3, or the in-crate test where
  `RecordingGateway` makes it nearly free, can add a key near the cap (about 8,388,000 bytes,
  or at least one over 131,072). The brief did accept 70,000 bytes as enough for criterion
  (3), so declining with that recorded reason is also a valid answer.

- **The C4-verify summary in `check-gates.json` overstates the evidence. This is a harness
  wording problem, not a defect in this patch.** The `path_line` says "5 test(s) ran red", but
  `gate-logs/C4-verify.log` shows `2 passed; 3 failed` on the reverted base. 3 red and 2 green
  is the correct, intended result: legs 3 and 4 are designed to pass both before and after the
  fix. The proof is sound; the harness's count is wrong.

- **The T4-batch-review row: I can't confirm the review ran on this diff.** The log is one
  line ("0 blocking …") and the row took 13.8 s for "3x codex" passes. If that was a cache hit
  from an earlier diff, the rubric's "one deep, multi-pass review" has not happened for this
  round. A human can check `review-batch.md`. This is informational only; I have no evidence
  of a problem.

## Refutation attempts that failed (evidence for the fix)

- **Red→green re-run** (scratch copy, toolchain present). With the fix, all 5 tests in
  `s3_key_length_limit` pass. With production `lib.rs` reverted and the test kept, legs 1, 2
  and 5 fail at `crates/server/tests/s3_key_length_limit.rs:247` (`200` where `400` was
  expected), and legs 3 and 4 pass. That matches the frozen log and the brief. The test
  drives the production router over a real loopback socket (not a copy of production code),
  and it seeds through the real `Gateway::put_object`.
- **Alternate spellings of an over-limit PUT** against the guards at
  `crates/gateway-s3/src/lib.rs:1851-1869`:
  - `//b/<1025>` → `400 KeyTooLongError` (`split_bucket_key`, `:590-596`, trims every
    leading `/`, so the key is still measured)
  - `/%62/<1025>` → `400 KeyTooLongError`
  - `?x-id=PutObject` → `400 KeyTooLongError`
  - `/b%2F/<1024 bytes>` → `400 InvalidBucketName`
  - `/b//<1023>` → `200` (correct: a 1,024-byte key `/yyy…`), and `/b//<1024>` → `400`
  - 256 × 😀 (1,024 bytes) → `200`, and one more byte → `400`

  No bypass. The guard runs before the payload `match` (`:1922`), so aws-chunked, signed and
  unsigned bodies all reach it, and before any body byte is read.
- **Measuring the wrong thing.** Each wrong way of measuring the key turns a specific leg red.
  Measuring the raw wire key fails the fully percent-encoded 1,024-byte key and 512 × `é`.
  Counting characters fails 342 × `€`. Measuring `bucket/key` fails the 1,024-byte acceptance.
  `cargo mutants` caught 5 of 5 mutants.
- **The new doc claims.** "The body cap is the ONLY limit on one `<Key>`" (`:518-525`, and
  `05-building-block-view.md`): confirmed up to 8,388,000 bytes, as above. "`414` past 65,534
  bytes" (`:498`, `:522-523`): confirmed. A 65,531-byte key (a 65,534-byte request target)
  gets HEAD `200` / DELETE `204`; a 65,532-byte key gets `414` / `414`. No other write
  surface exists: `crates/proto` has no object-key RPC, and the CLI's
  `crates/server/src/cli.rs:1905` goes through `Gateway::put_object`.
- **Not a finding (older behaviour, out of this diff's scope):** `PUT /b/%FF×342`, which is
  342 raw bytes of invalid UTF-8, gets `KeyTooLongError`. `percent_decode_utf8`
  (`:3365-3383`) decodes lossily, turning each bad byte into the 3-byte replacement character
  U+FFFD. The measured key is the key that would have been stored, and S3 rejects invalid
  UTF-8 keys anyway.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. The diff is clean on both advisory lenses: introduced correctness bugs and actionable reuse, simplification, or efficiency issues.

Reviewed the PUT guards (`crates/gateway-s3/src/lib.rs:1851`, `crates/gateway-s3/src/lib.rs:1862`), write-seam assertions (`crates/gateway-s3/src/lib.rs:4770`), and wire regressions (`crates/server/tests/s3_key_length_limit.rs:256`). The boundary checks measure the decoded key's UTF-8 bytes; deletion tests verify actual removal of previously present objects.

Validation used the frozen gate evidence, without rerunning builds: CI passed; all five wire tests passed with the fix; the three expected creation tests failed when production changes were reverted; all five mutants were caught.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Confirm that the creation-only S3 compatibility boundary satisfies #620’s user needs — automated checks establish the specified wire behavior, while recovery beyond the unchanged bulk-body cap remains outside this repair’s scope; `brief.md:123`, `docs/design/architecture/05-building-block-view.md:139`.
- [ ] **Leg 3 only protects keys up to 70,000 bytes.** The brief's Invariant and the patch's own doc (`crates/gateway-s3/src/lib.rs:498-500`, `:518-525`) promise that bulk delete has **no** length bound below the 8 MiB body cap. The tests only check that promise up to the 70,000-byte fixture (`crates/server/tests/s3_key_length_limit.rs:328`, `crates/gateway-s3/src/lib.rs:4787`). Concrete failing case: in a scratch copy I added `if key.len() > 100_000 { return Err(DeleteRequestError::Malformed); }` right after the empty-key check in `parse_delete_request` (`crates/gateway-s3/src/lib.rs:2331-2333`). That mutant passed every test: `s3_key_length_limit` (5/5), all 82 `wyrd-gateway-s3` lib tests, and `s3_delete_objects` (22/22). Yet it strands every key of 100,001 to 131,072 bytes that `wyrd put --key` can write. Those keys are past the `414` limit, so bulk delete is their only S3 exit, which is exactly the iteration-4 sign-off defect. The fix is cheap. Over the real wire, bulk delete of an 8,388,000-byte key seeded through `Gateway::put_object` works today (200, `<Deleted>`, no `<Error>`, gone afterwards), and three such deletes (131 KB, 3 MB, 8.4 MB) took about 1 s in total. So leg 3, or the in-crate test where `RecordingGateway` makes it nearly free, can add a key near the cap (about 8,388,000 bytes, or at least one over 131,072). The brief did accept 70,000 bytes as enough for criterion (3), so declining with that recorded reason is also a valid answer.
- [ ] **The deletion guarantee exceeds the deliberately retained limits.** `brief.md:33` promises “Bulk delete reaches every stored key, whatever its length,” and `brief.md:67-70` makes universal deletability an invariant, while `brief.md:112-113` excludes changing the 8 MiB body cap and request-target ceiling. On `$PDCA_TARGET`, `crates/gateway-s3/src/lib.rs:507` sets that cap; `:1920-1921` rejects an overflowing body; and `:2007-2009` returns `400 MalformedXML` before deletion. The native fixture API adds no corresponding key-length check (`crates/server/src/lib.rs:235-256`, `:269-284`; `crates/core/src/metadata.rs:39-40`). A native-seeded `b/<a × 8,388,609>` needs at least 8,388,654 bytes in a one-key DeleteObjects body, so the retained handler refuses it; single-object DELETE also exceeds the brief's stated URI ceiling. The 70,000-byte fixture proves only a case below the body cap. This boundary is also explicit in the maintainer comment (`notes.json:1`): a single key is “bounded only by `MAX_DELETE_BODY_BYTES` (8 MiB).” Revise criterion (3), the invariant, and the planned documentation to promise preservation of existing deletion reachability, with no new 1,024-byte refusal, subject to existing request-format and size limits. Acknowledge the remaining recovery limitation alongside the native-limit deferral; retaining the universal promise would require additional scope beyond this PUT fix.
- [ ] **The patch strands objects whose keys are too long for a request line.** `crates/gateway-s3/src/lib.rs:2266` now refuses any bulk-delete `<Key>` over 1,024 bytes. The patch relies on single-object DELETE (`crates/gateway-s3/src/lib.rs:1917`) as the way out for *every* over-limit object (the claims at `:493`, `:1818`, `:1978` and `crates/server/tests/s3_key_length_limit.rs:19`). That way out has a ceiling. The gateway runs on `axum::serve` with hyper defaults (`crates/gateway-s3/src/lib.rs:279`). Hyper parses the request target with the `http` crate (`Cargo.lock:1608-1609`, 1.4.2), whose `Uri` rejects anything over 65,534 bytes, and hyper answers `414` before the gateway runs. **Concrete failing case:** seed `b/<'a' × 70,000>` through `Gateway::put_object`, as leg 4 does. On the patched code, HEAD is `414`, DELETE is `414`, bulk delete is `400 KeyTooLongError`, and the object is still present. On the pre-fix base, HEAD and DELETE are the same `414`s, but bulk delete answers `200` and the object is gone. Such keys can exist. `wyrd put --key` (`crates/server/src/cli.rs:562-568`) takes the key from argv (up to 128 KiB per argument on Linux) into the same metadata store. The CLI has no `delete` subcommand (`crates/server/src/cli.rs:415-416`), so S3 bulk delete was the only exit. Keys that need percent-encoding hit the ceiling sooner, at about 21,800 key bytes, because each byte becomes 3 wire bytes. Leg 4 seeds only a 1,025-byte key (`s3_key_length_limit.rs:319`), so it can't see this. Tagged `[human]` because the builder did what the brief said: criterion (3) demands the bulk refusal, while the invariant and C-1 demand that nothing becomes undeletable, and the two conflict above the URI ceiling. Options: drop the per-entry bound from bulk delete and keep the body cap; raise the bound above what a single DELETE can address; or accept the gap and record it (and/or apply the 1,024-byte limit to `wyrd put --key`, which the brief left out of scope).
- [ ] Refusing the whole bulk-delete request (`crates/gateway-s3/src/lib.rs:2270`, `:2114`) makes bucket-emptying tools fail on any batch that contains a legacy over-limit key. Concrete case, run as a probe against the patched tree: I seeded `b/<'a' × 1025>` through `Gateway::put_object` and PUT `k1`, `k2`, `k3`. ListObjectsV2 returns all 4 keys, including the long one. A `DeleteObjects` built from that listing answers `400 KeyTooLongError`, and `k1`, `k2` and `k3` all survive. Any client that lists keys and then deletes them in batches (`aws s3 rm --recursive`, `mc rm -r`, `rclone purge`) fails for every 1,000-key batch holding such a key, not just for that one key. The object itself is not stranded, because single-object `DELETE` still removes it (leg 4), so the brief's invariant holds as written. The patch does what criterion (3) asks. Whether whole-request refusal is the right design, or whether a per-key `<Error><Code>KeyTooLongError</Code>` row (or no bound at all) would be better, is a brief-level call. This probably overlaps the round-1 finding already held for the human ("moot if the human drops the bulk bound").
- [ ] **One stored over-limit key blocks bulk cleanup of up to 999 other keys.** `crates/gateway-s3/src/lib.rs:2270` (with `:2114`) refuses a whole DeleteObjects (S3 bulk delete) request if any entry is over 1,024 bytes. Over-limit objects can still exist: those stored before this fix, or written with `wyrd put --key`. `list_container` lists them without filtering (`crates/server/src/lib.rs:714-741`). Concrete case:

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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — **Leg 3 only protects keys up to 70,000 bytes.** The brief's Invariant and the patch's own doc (`crates/gateway-s3/src/lib.rs:498-500`, `:518-525`) promise that bulk delete has **no** length bound below the 8 MiB body cap. The tests only check that promise up to the 70,000-byte fixture (`crates/server/tests/s3_key_length_limit.rs:328`, `crates/gateway-s3/src/lib.rs:4787`). Concrete failing case: in a scratch copy I added `if key.len() > 100_000 { return Err(DeleteRequestError::Malformed); }` right after the empty-key check in `parse_delete_request` (`crates/gateway-s3/src/lib.rs:2331-2333`). That mutant passed every test: `s3_key_length_limit` (5/5), all 82 `wyrd-gateway-s3` lib tests, and `s3_delete_objects` (22/22). Yet it strands every key of 100,001 to 131,072 bytes that `wyrd put --key` can write. Those keys are past the `414` limit, so bulk delete is their only S3 exit, which is exactly the iteration-4 sign-off defect. The fix is cheap. Over the real wire, bulk delete of an 8,388,000-byte key seeded through `Gateway::put_object` works today (200, `<Deleted>`, no `<Error>`, gone afterwards), and three such deletes (131 KB, 3 MB, 8.4 MB) took about 1 s in total. So leg 3, or the in-crate test where `RecordingGateway` makes it nearly free, can add a key near the cap (about 8,388,000 bytes, or at least one over 131,072). The brief did accept 70,000 bytes as enough for criterion (3), so declining with that recorded reason is also a valid answer.. Deferred, not addressed here: 1 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-18

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
