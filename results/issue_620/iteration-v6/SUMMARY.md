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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 92.2% — 166 of 180 instrumentable changed lines executed (floor 80%); 180 of 317 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 5 mutants tested in 25s: 5 caught

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_620/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.73s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect found in #620’s fix to enforce S3’s 1,024-byte creation limit while preserving deletion of existing oversized keys.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The five acceptance criteria distinguish decoded UTF-8 bytes, encoded-path aliases and legacy deletion, with explicit observable outcomes and scope limits (`brief.md:24`, `brief.md:115`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the fix reproduced all three expected creation failures: HTTP 200 instead of 400; all three deletion tests remained green (`reviewer-red.log:6`, `reviewer-red.log:44`). |
| C3 Change | PASS | The behavioral change stays within the agreed creation boundary and preserves the existing deletion contract; the living architecture records both the new policy and residual request limits (`crates/gateway-s3/src/lib.rs:1851`, `docs/design/architecture/05-building-block-view.md:139`). |
| C4 Verification (red→green) | PASS | Restoring the fix independently made all six wire tests pass; frozen CI passes, and both local advisory audits pass after relocating their read-only cache (`reviewer-restored-green.log:14`, `gate-logs/C4-ci.log:4233`, `reviewer-deny.log:1`, `reviewer-deny-all-features.log:1`). |
| C5 Causal adequacy | PASS | Creation-boundary validation addresses the demonstrated cause, and the recording backend checks that refused names never reach a write; no optional-capability or load-time fallback is introduced (`crates/gateway-s3/src/lib.rs:1862`, `crates/gateway-s3/src/lib.rs:4770`). |
| T1 Structure | PASS | S3 policy remains in the protocol gateway behind the existing narrow traits; no dependency, global state, production clock or destructive path is added (`crates/gateway-s3/src/lib.rs:1669`, `crates/gateway-s3/src/lib.rs:1851`, `crates/server/tests/s3_key_length_limit.rs:41`). |
| T2 Shape | PASS | Boundary coverage distinguishes 1,024 decoded bytes from characters, encoded bytes and the bucket prefix, and checks the encoded-bucket bypass against actual stored identity (`crates/server/tests/s3_key_length_limit.rs:313`, `crates/server/tests/s3_key_length_limit.rs:502`). |
| T3 Runtime | PASS | Real socket tests confirm existing objects remain removable through single and bulk deletion, including an 8,388,563-byte key; exchanges have deadlines and the server task aborts on drop (`crates/server/tests/s3_key_length_limit.rs:426`, `crates/server/tests/s3_key_length_limit.rs:477`, `crates/server/tests/s3_key_length_limit.rs:97`). |
| T4 Contribution | N/A | Commit/PR artifacts are intentionally drafted after Check; their substantive audit must rerun at publish. Prior art was independently checked by all affected paths, including closed/rejected work (`gate-logs/T4-contribution.log:10`, `reviewer-prior-art.log:1`). |
| T5 Judgment | PASS | The prior 70,000-byte-only coverage concern is addressed by an exact-body-cap regression, while the accepted creation-only scope and existing native-limit deferral remain intact (`brief.md:200`, `crates/server/tests/s3_key_length_limit.rs:428`, `docs/design/architecture/05-building-block-view.md:139`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated creation policy and preserved legacy deletion satisfy #620 for release — recovery of API-created keys exceeding the existing body cap remains outside the agreed scope (`brief.md:115`, `docs/design/architecture/05-building-block-view.md:139`). |

Source paths refer to the supplied `$PDCA_TARGET`; brief and evidence paths refer to this bundle. The target is readable, the supplied patch reverse-checks cleanly, and the original patched state was restored after the independent reproduction.

The evidence supports three conclusions:

- **The fix closes the demonstrated creation hole.** The same signed-wire suite compiled against both trees. The pre-fix failures were the 1,025-byte key, the 1,026-byte UTF-8 key and the encoded bucket separator; restoration produced six passes (`reviewer-red.log:14`, `reviewer-restored-green.log:6`). The byte limit and error match the [AWS object-key contract](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-keys.html) and [AWS error reference](https://docs.aws.amazon.com/AmazonS3/latest/API/API_Error.html).
- **Legacy deletion remains exercised through its existing body limit.** The exact-cap wire test seeds a real object, verifies its presence, requires its precise `<Deleted>` row with no `<Error>`, and verifies its absence afterward (`crates/server/tests/s3_key_length_limit.rs:426`). The corresponding gateway test checks the exact backend key (`crates/gateway-s3/src/lib.rs:4851`). This discharges the carried-forward test weakness without imposing a new bulk-delete refusal. Keys exceeding the existing body cap are the explicitly documented residual, not a newly introduced defect.
- **The review found no outstanding implementation or dependency concern.** The standing rubric was checked for protocol boundaries, narrow seams, clocks, mutable globals, unsafe code, documentation currency and test lifecycle. Prior-art queries inspected 37 gateway commits, 36 architecture commits, the new test path’s empty history, and the full file lists of all 19 unmerged PRs among 396 closed PRs; none of those unmerged PRs touched an affected path (`reviewer-prior-art.log:2`). Merged [PR #612](https://github.com/getwyrd/wyrd/pull/612) and the brief’s rejected bulk-refusal design were considered.

The frozen auxiliary evidence is available and was read: coverage executed 166/180 instrumentable changed lines, with 137 lines unscored (`gate-logs/C4-diff-cov.log:137`, `gate-logs/C4-diff-cov.log:164`); mutation testing caught 5/5 mutants (`gate-logs/C5-mutants.log:13`); the batched review reports zero findings (`gate-logs/T4-batch-review.log:10`); and both TiKV feature compilations completed (`gate-logs/host-tikv.log:111`, `gate-logs/host-tikv.log:209`). These are frozen-log results, not claims that their instance-scoped wrappers were rerun here. The contribution check is deferred, not missing evidence.

The independent full `cargo xtask ci` rerun completed with 1,886 passing test executions and zero test failures; formatting, clippy, builds, workspace/DST tests, documentation checks, dependency-use scanning, conformance and repository guards passed. Its aggregate exit was **1**, solely because both advisory checks tried to lock the read-only shared cache (`reviewer-ci.log:3599`, `reviewer-ci.log:4227`). Both checks then passed with the real cargo-deny tool and fetched advisory database using a writable cache under this bundle (`reviewer-deny.log:1`, `reviewer-deny-all-features.log:1`). Parsed configuration comparison confirmed that only `advisories.db-path` changed in the scratch copies; target policies were untouched. Thus the brief’s `cargo deny all-features advisories`, `typos` and `docs-renderer` dependencies were exercised, with no remaining dependency decision owed.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 620 (s3-object-key-length-limit), iteration 6

Bottom line: I could not break the fix. The production change is correct on every input I tried. I found one weak spot in the tests (below, tagged `[impl]`, low severity). Everything else is a refutation attempt that failed.

## Finding

- NEEDS-HUMAN [impl] — **The tests only send signed single-shot PUTs, so they don't pin the limit for the PUT form the stock SDK actually uses.** Every new PUT leg goes through `signed()` (`crates/server/tests/s3_key_length_limit.rs:162`, `:175`) or `signed_key_limit_request` (`crates/gateway-s3/src/lib.rs:4666`, `:4683`). Both compute a signed body hash, so every request lands in the `PayloadHash::Signed` arm. Concrete failing case: in a scratch copy I moved the `key_name.len() > MAX_KEY_BYTES` check (`crates/gateway-s3/src/lib.rs:1862-1869`) inside the `sigv4::PayloadHash::Signed(hex) =>` arm (`:1944`). With that change, all 6 wire tests and all 3 in-crate #620 tests still pass. Yet under that change an `aws-chunked` PUT (`STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, the stock SDK default, `:1929`) or an `UNSIGNED-PAYLOAD` PUT (`:1957`) stores a 1,025-byte key. That breaks the Invariant ("no object whose S3 key exceeds 1,024 UTF-8 bytes can be created through the S3 surface"). The same gap covers the `InvalidBucketName` check (`:1851`). The **shipped code is correct**: it runs both checks before the payload `match`. I confirmed this with a scratch probe: aws-chunked 1,025-byte PUT → `400 KeyTooLongError`, HEAD `404`; UNSIGNED-PAYLOAD 1,025-byte PUT → `400 KeyTooLongError`, HEAD `404`; aws-chunked `PUT /b%2Fa/<'x'×1023>` → `400 InvalidBucketName`. So this is only a missing regression guard. It matters because the PUT arm keeps getting header-time refusals added to it (#671 already landed there). The fix is cheap: add one aws-chunked leg (or one UNSIGNED-PAYLOAD leg) for the 1,025-byte key, reusing the signer and framer helpers already in `crates/server/tests/s3_put_object_ceiling.rs:156-272`. Declining it with a recorded reason is also fine, given the production code is right.

## Refutation attempts that failed (no action needed)

- **Red→green evidence holds, and it runs the production path.** I re-ran it myself in a scratch copy of `$PDCA_TARGET`. Patched tree: 6/6 green. With `crates/gateway-s3/src/lib.rs` restored to base (`git show HEAD:`) and the new test kept: the 3 PUT legs fail at `crates/server/tests/s3_key_length_limit.rs:261` (`assert_refused`, `200` vs `400`). The 3 delete legs stay green, as the brief's Falsifiability section says they should. This matches `gate-logs/C4-verify.log` exactly. The test drives the real loopback hyper listener, real SigV4 verification, the real router and redb. It is not a copy of production logic.
- **The iteration-5 sign-off finding is fixed.** I added `if key.len() > 8_388_000 { return Err(DeleteRequestError::Malformed); }` after the empty-key check in `parse_delete_request` (`crates/gateway-s3/src/lib.rs:2331-2334`). Both `bulk_delete_removes_the_longest_key_the_body_cap_can_carry` (`crates/server/tests/s3_key_length_limit.rs:426`) and the in-crate `a_bulk_delete_deletes_the_longest_key_the_body_cap_can_carry` (`crates/gateway-s3/src/lib.rs:4826`) go red. With a bound of `1024` instead, leg 3 (`:350`) and both in-crate delete tests go red. So a bulk-delete length bound anywhere below the cap is now caught.
- **The `414` claims in the docs are true.** Scratch probe of HEAD on the live gateway: a 65,534-byte request target reaches the gateway (`404`); a 65,535-byte one and a 70,003-byte one get `414`. That backs `crates/gateway-s3/src/lib.rs:498-500`, `:518-525` and `docs/design/architecture/05-building-block-view.md:139`.
- **I found no way to spell the path that gets around the limit.** Lowercase `%2f` in the bucket decodes to `/` (`to_digit(16)`, `crates/gateway-s3/src/lib.rs:3371-3372`), so it is refused. `/%2F/k` and `/b%2F/k` are refused too. A double-encoded `%252F` decodes once, to a literal `%2F`, which names a different bucket, not `b`. `//b/k` folds to bucket `b` (`:591`), but the measured key equals the stored key. `/b//k` measures `/k`, the same thing it stores. Lossy UTF-8 decoding (`:3382`) happens before the length check, so the bytes measured are exactly the bytes stored: there is no gap between what is measured and what is written.
- **No other S3 path creates objects.** The only calls into the write seam are the three `put_object_streaming` arms (`crates/gateway-s3/src/lib.rs:1933`, `:1947`, `:1960`), and all three sit below the check. CopyObject (`:1831`) and the multipart and subresource forms (`:1808`) are refused before the check runs.
- **The claim in `check-gates.json` is worded too strongly, but this isn't the builder's fault.** The C4-verify row says "6 test(s) ran red". In the red run only 3 of the 6 failed (`gate-logs/C4-verify.log`: "3 passed; 3 failed"). The other 3 are green on both trees by design. The verdict ("red without the fix") still holds. Don't read it as "all 6 legs tell fixed from unfixed".

Scratch work was under `$PDCA_SCRATCH/pdca-adversary-620-run` and has been removed.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness findings. The PUT guards check decoded bucket/key values before body streaming (`crates/gateway-s3/src/lib.rs:1851`, `crates/gateway-s3/src/lib.rs:1862`). The new tests verify refusal without storage, UTF-8 byte boundaries, and actual deletion of legacy keys, including a request exactly at the body cap (`crates/server/tests/s3_key_length_limit.rs:281`, `crates/server/tests/s3_key_length_limit.rs:426`).
- No actionable reuse, simplification, or efficiency findings. The changes reuse the existing percent-decoder, error-response builder, and SigV4 encoding/signing helpers (`crates/gateway-s3/src/lib.rs:1798`, `crates/gateway-s3/src/lib.rs:4666`, `crates/server/tests/s3_key_length_limit.rs:223`).

Validation relied on the frozen gate logs: all six wire tests passed; reverting production changes caused the three expected creation tests to fail; all five tested mutants were caught. No builds were rerun or target files modified.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated creation policy and preserved legacy deletion satisfy #620 for release — recovery of API-created keys exceeding the existing body cap remains outside the agreed scope (`brief.md:115`, `docs/design/architecture/05-building-block-view.md:139`).
- [ ] **The tests only send signed single-shot PUTs, so they don't pin the limit for the PUT form the stock SDK actually uses.** Every new PUT leg goes through `signed()` (`crates/server/tests/s3_key_length_limit.rs:162`, `:175`) or `signed_key_limit_request` (`crates/gateway-s3/src/lib.rs:4666`, `:4683`). Both compute a signed body hash, so every request lands in the `PayloadHash::Signed` arm. Concrete failing case: in a scratch copy I moved the `key_name.len() > MAX_KEY_BYTES` check (`crates/gateway-s3/src/lib.rs:1862-1869`) inside the `sigv4::PayloadHash::Signed(hex) =>` arm (`:1944`). With that change, all 6 wire tests and all 3 in-crate #620 tests still pass. Yet under that change an `aws-chunked` PUT (`STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, the stock SDK default, `:1929`) or an `UNSIGNED-PAYLOAD` PUT (`:1957`) stores a 1,025-byte key. That breaks the Invariant ("no object whose S3 key exceeds 1,024 UTF-8 bytes can be created through the S3 surface"). The same gap covers the `InvalidBucketName` check (`:1851`). The **shipped code is correct**: it runs both checks before the payload `match`. I confirmed this with a scratch probe: aws-chunked 1,025-byte PUT → `400 KeyTooLongError`, HEAD `404`; UNSIGNED-PAYLOAD 1,025-byte PUT → `400 KeyTooLongError`, HEAD `404`; aws-chunked `PUT /b%2Fa/<'x'×1023>` → `400 InvalidBucketName`. So this is only a missing regression guard. It matters because the PUT arm keeps getting header-time refusals added to it (#671 already landed there). The fix is cheap: add one aws-chunked leg (or one UNSIGNED-PAYLOAD leg) for the 1,025-byte key, reusing the signer and framer helpers already in `crates/server/tests/s3_put_object_ceiling.rs:156-272`. Declining it with a recorded reason is also fine, given the production code is right.
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
- Iteration delta (if iterating): Auto-iterate (round 5): rebuilding for the implementation-level findings — **The tests only send signed single-shot PUTs, so they don't pin the limit for the PUT form the stock SDK actually uses.** Every new PUT leg goes through `signed()` (`crates/server/tests/s3_key_length_limit.rs:162`, `:175`) or `signed_key_limit_request` (`crates/gateway-s3/src/lib.rs:4666`, `:4683`). Both compute a signed body hash, so every request lands in the `PayloadHash::Signed` arm. Concrete failing case: in a scratch copy I moved the `key_name.len() > MAX_KEY_BYTES` check (`crates/gateway-s3/src/lib.rs:1862-1869`) inside the `sigv4::PayloadHash::Signed(hex) =>` arm (`:1944`). With that change, all 6 wire tests and all 3 in-crate #620 tests still pass. Yet under that change an `aws-chunked` PUT (`STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, the stock SDK default, `:1929`) or an `UNSIGNED-PAYLOAD` PUT (`:1957`) stores a 1,025-byte key. That breaks the Invariant ("no object whose S3 key exceeds 1,024 UTF-8 bytes can be created through the S3 surface"). The same gap covers the `InvalidBucketName` check (`:1851`). The **shipped code is correct**: it runs both checks before the payload `match`. I confirmed this with a scratch probe: aws-chunked 1,025-byte PUT → `400 KeyTooLongError`, HEAD `404`; UNSIGNED-PAYLOAD 1,025-byte PUT → `400 KeyTooLongError`, HEAD `404`; aws-chunked `PUT /b%2Fa/<'x'×1023>` → `400 InvalidBucketName`. So this is only a missing regression guard. It matters because the PUT arm keeps getting header-time refusals added to it (#671 already landed there). The fix is cheap: add one aws-chunked leg (or one UNSIGNED-PAYLOAD leg) for the 1,025-byte key, reusing the signer and framer helpers already in `crates/server/tests/s3_put_object_ceiling.rs:156-272`. Declining it with a recorded reason is also fine, given the production code is right.. Deferred, not addressed here: 1 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-18

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
