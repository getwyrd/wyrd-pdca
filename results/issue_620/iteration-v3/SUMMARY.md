# Result — issue 620 / s3-object-key-length-limit

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The S3 gateway enforces S3's 1,024-byte object-key limit nowhere. A `PUT
  /{bucket}/{key}` stores a key of any length: the decoded key is composed at
  `crates/gateway-s3/src/lib.rs:1765-1769` and streamed straight into `put_object_streaming`
  (`:1829-1868`). `parse_delete_request` (`:2171-2245`) accepts any non-empty `<Key>`
  (`:2209-2212`) up to the whole 8 MiB body cap. Yet `MAX_DELETE_BODY_BYTES` (`:488-507`) is sized
  on "1000 keys × a 1024-byte key", so the cap and the parser disagree about what one entry can
  be (the #612 review follow-up in this issue's thread). The limit cannot land in bulk delete
  alone. PUT would still create a 2,000-byte key, and bulk delete would then refuse to delete
  it, which is the "undeletable object" this issue was deferred to avoid.
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
  (3) **Bulk delete's per-entry bound matches.** A `DeleteObjects` request naming a 1,025-byte key
  next to an existing ordinary key is refused as a whole with `400` `KeyTooLongError`, and no key
  is touched: the ordinary object still exists afterwards. A request whose keys are all ≤ 1,024
  bytes, including one of exactly 1,024 bytes, behaves as it does today.
  (4) **Deletion never strands an object.** An over-limit object that already exists stays
  removable by single-object `DELETE`. The test seeds `b/<'a' × 1025>` through the existing
  backend API, `Gateway::put_object` (`crates/server/src/lib.rs:235`), on an `Arc<Gateway>` clone
  it keeps before handing the gateway to `S3Gateway::new` (`crates/gateway-s3/src/lib.rs:232`
  takes `Arc<G>`). Then: signed HEAD of that key is `200`; signed `DELETE` is `204`; HEAD again
  is `404`. The HEAD-before and HEAD-after legs are required, because DELETE answers `204` for
  an absent key too (`crates/gateway-s3/src/lib.rs:1887-1889`), so `204` alone proves nothing.
  This is the way out for any over-limit object stored before this fix or written outside the S3
  surface (the data-plane `wyrd put --key`, `crates/server/src/cli.rs:562-568`).
  (5) **No bucket-separator alias around the limit.** The gateway splits the path on the first
  literal `/` before decoding (`crates/gateway-s3/src/lib.rs:567-572`) and joins the decoded
  bucket and key into one flat key (`:1765-1769`). So `PUT /b%2Fa/<'x' × 1023>` has a 1,023-byte
  request key yet stores the same object as `/b/a/<'x' × 1023>`, whose key under bucket `b` is
  1,025 bytes. A PUT whose decoded bucket segment contains `/` must be refused `400`
  `InvalidBucketName` (S3 bucket names cannot contain `/`) before the body is stored: afterwards
  HEAD of `/b/a/<'x' × 1023>` is `404`. Send this one as signed raw HTTP (the SDK will not put
  `%2F` in a bucket name).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: S3 key-length enforcement where an object is created (PUT), the refusal of a PUT
  whose decoded bucket segment contains `/` (criterion 5), and the per-entry bound
  in the bulk-delete parser, with the `MAX_DELETE_BODY_BYTES` sizing comment kept true / out of
  scope: CopyObject, which answers `501` today (`crates/gateway-s3/src/lib.rs:1800-1806`; when
  #504 implements it, the same rule applies to its destination key); single-object GET, HEAD and
  DELETE (unchanged, see criterion 4); listing parameters (`prefix`, `start-after`,
  `continuation-token`); the data-plane CLI `wyrd put --key`, which is not an S3 surface; bucket
  naming rules, except the one criterion (5) needs (no `/` in a decoded bucket, on PUT only; GET,
  HEAD and DELETE on such a path stay as they are, since they create nothing); the multipart verbs (#508's slices apply the same rule to
  CreateMultipartUpload's key when they land); the 8 MiB cap value itself.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 94.4% — 170 of 180 instrumentable changed lines executed (floor 80%); 180 of 313 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 12 mutants tested in 39s: 11 caught, 1 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_620/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.37s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #620: the S3 1,024-byte object-key limit satisfies the brief; baseline dependency-gate disposition and fitness-to-purpose remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The five acceptance cases distinguish decoded UTF-8 bytes, bucket aliases, whole-request refusal, and legacy-object removal, with observable outcomes (`brief.md:19`; `crates/server/tests/s3_key_length_limit.rs:240`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the production fix while retaining the new test produces four assertion failures on erroneous HTTP 200 responses; legacy deletion already passes (`reviewer-evidence/red.log:268`, `reviewer-evidence/red.log:313`). |
| C3 Change | PASS | The three-file patch stays within the agreed gateway, regression-test, and living-documentation scope; the documented compatibility behavior matches the implementation (`crates/gateway-s3/src/lib.rs:1828`, `crates/gateway-s3/src/lib.rs:2270`; `docs/design/architecture/05-building-block-view.md:137`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide disposition of the unchanged TiKV dependency-policy failure under #543 — independent red→green passes, but both audits reject RUSTSEC-2026-0346/0349, so the full gate remains red; the supplementary DST sweep is incomplete (`reviewer-evidence/green.log:15`; `reviewer-evidence/deny-all-advisories.log:10`, `reviewer-evidence/deny-all-advisories.log:40`; `deny-all-features.toml:80`). |
| C5 Causal adequacy | PASS | Boundary validation prevents the invalid creation and bucket-alias causes while preserving legacy deletion; this is input validation, with no capability-probe/load-time workaround (`crates/gateway-s3/src/lib.rs:1828`, `crates/gateway-s3/src/lib.rs:1838`, `crates/gateway-s3/src/lib.rs:1920`; `reviewer-evidence/green.log:11`). |
| T1 Structure | PASS | Validation remains at the S3 translation boundary and finishes before destructive fan-out, preserving the existing trait seams (`crates/gateway-s3/src/lib.rs:1851`, `crates/gateway-s3/src/lib.rs:2110`, `crates/gateway-s3/src/lib.rs:2270`). |
| T2 Shape | PASS | One byte limit and one error response keep PUT and bulk-delete semantics consistent; tests cover exact limits, XML expansion, and multibyte input (`crates/gateway-s3/src/lib.rs:497`, `crates/gateway-s3/src/lib.rs:2023`, `crates/gateway-s3/src/lib.rs:4545`). |
| T3 Runtime | PASS | Real loopback tests demonstrate refusal without storage/deletion and successful legacy removal; socket exchanges are bounded and the owned server is aborted on drop (`reviewer-evidence/green.log:9`; `crates/server/tests/s3_key_length_limit.rs:66`, `crates/server/tests/s3_key_length_limit.rs:83`, `crates/server/tests/s3_key_length_limit.rs:355`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their substantive audit must rerun at publish; the deferred row requires no human clearance (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | No unresolved implementation concern remains: the strengthened byte-boundary tests discriminate mutations, and path-based prior-art checks found no conflicting rejected work (`reviewer-evidence/mutants-cap-lints.log:6`; `reviewer-evidence/prior-art-summary.log:1`, `reviewer-evidence/prior-art-summary.log:6`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the intended compatibility tradeoff — a legacy over-limit key now refuses an entire bulk-delete batch, while individual deletion remains available; decide whether that meets operator/client needs (`brief.md:29`; `docs/design/architecture/05-building-block-view.md:137`; `crates/server/tests/s3_key_length_limit.rs:355`). |

No patch defect was found. Source citations above resolve in `$PDCA_TARGET`, the harness's disposable copy. Its patch applied cleanly; the stash was restored and reverse-application and whitespace checks passed. No implementation edits were made.

The dependency-policy failure is independently confirmed, and remains separate from patch correctness. The frozen full CI run reaches the all-features advisory audit and rejects the unchanged TiKV → tonic → rustls 0.21.12 chain (`gate-logs/C4-ci.log:6483`; `Cargo.lock:3294`). The local full run passes typos, docs lint/render, repository guards, formatting, clippy, build, workspace tests, and dependency-use scanning, then cannot lock the read-only advisory cache (`reviewer-evidence/ci.log:4`, `reviewer-evidence/ci.log:3268`). Copying that database into this sandbox and changing only its path in scratch policy copies reproduces the same two advisory failures; default auditing and all-features licenses/bans/sources pass (`reviewer-evidence/deny-all-advisories.log:3`; `reviewer-evidence/deny-default.log:4`; `reviewer-evidence/deny-all-other.log:4`). This uses the real scanner and database, with no advisory suppression or substituted tool. The `typos` external dependency was exercised.

The patch's behavioral evidence is independently reproduced:

- **C4-verify: PASS.** Four failures and one pass before the fix become five passes after restoration. The 80 gateway tests and 46 neighboring bulk-delete/listing tests also pass (`reviewer-evidence/red.log:313`; `reviewer-evidence/green.log:15`; `reviewer-evidence/gateway.log:110`; `reviewer-evidence/wire-peers.log:31`, `reviewer-evidence/wire-peers.log:61`).
- **C5-mutants: PASS.** All 11 compilable mutants are caught; one is unviable. The first run had five additional mutations rejected by unused-code lints, so a second mutation-only run capped lints at warnings to exercise their behavioral assertions (`reviewer-evidence/mutants.log:5`; `reviewer-evidence/mutants-cap-lints.log:3`, `reviewer-evidence/mutants-cap-lints.log:6`).
- **C4-diff-cov: PASS from frozen evidence.** Its instance-scoped wrapper is unavailable here; the log records both test suites and 170/180 instrumentable changed lines covered (94.4%). Its stated boundary excludes 133 unscored lines and includes co-located test code, so this is not a claim of exhaustive production-path coverage (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:159`).
- **host-tikv: PASS.** Both real feature builds were independently recompiled with clippy: `wyrd-metadata-tikv --features tikv --tests` and `wyrd-server --features tikv,etcd --tests` (`reviewer-evidence/host-tikv.log:106`, `reviewer-evidence/host-tikv.log:208`), confirming the frozen compilation evidence.
- **T4-batch-review: PASS from frozen evidence.** The available log records zero blocking findings; it does not expose the individual reviewer reports (`gate-logs/T4-batch-review.log:10`). The separate contribution audit is **N/A until publish**, as recorded above.

Prior-art checking covered every affected path: merged history returned 33 gateway commits, zero commits for the new test, and 30 architecture-document commits. All 381 closed PRs were enumerated; the file lists of all 19 closed/unmerged PRs contained none of the affected paths. Merged PR #612 and issue #620 confirm the earlier deferral being addressed here (`reviewer-evidence/prior-art-summary.log:1`; `reviewer-evidence/prior-art-summary.log:6`). Existing tracked deferrals were not reopened.

Checks after the audit stopping point were also exercised separately: conformance vectors, the shared-state scanner, and the actual workspace orchestrator-import scanner passed (`reviewer-evidence/conformance.log:3`; `reviewer-evidence/statics.log:5`; `reviewer-evidence/deploy-guard.log:10`).

The supplementary full DST rerun was interrupted during the unchanged custodian regression-seed campaign after its clippy step and preceding test binaries passed; it supplies no complete DST verdict (`reviewer-evidence/dst.log:542`). A complete green `cargo xtask ci` is still owed after the dependency-policy failure is resolved. Neither the interrupted sweep nor the original read-only cache lock is classified as an S3 patch defect.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 620 (S3 object-key length limit)

**Bottom line:** I tried to refute the evidence and the fix and could not. The two NEEDS-HUMAN items below are a design question the brief settled on purpose, and a CI failure that sits outside this patch. I found no builder-fixable (`[impl]`) defect.

## The evidence (red→green), re-run independently

- **Red→green reproduces, on the production path.** I copied the target into scratch and ran `cargo test -p wyrd-server --test s3_key_length_limit`. With the fix: 5/5 pass. With `crates/gateway-s3/src/lib.rs` reverted to the base commit and the test kept: `1 passed; 4 failed`. All four fail at `crates/server/tests/s3_key_length_limit.rs:231` (`assert_refused`, which got `200`, not `400`). This matches `gate-logs/C4-verify.log`. The test drives the real `S3Gateway` router over a loopback socket against the real redb + fs backend (`s3_key_length_limit.rs:97-132` in the new file), so it is not a re-implementation that mirrors production. The negative checks can't pass on an empty or broken backend either: leg 1 requires an in-limit control object in the listing (`s3_key_length_limit.rs:260`), and leg 4 brackets the `204` with HEAD `200` before and HEAD `404` after (`s3_key_length_limit.rs:355`).
- **Hand-made mutations are all caught.** Each change below was applied to the fixed `lib.rs`, then I ran both the wire test and the crate's unit tests. Every one turned at least one test in each suite red:
  - bulk parser counts characters, not bytes (`lib.rs:2270`): this was the gap reported in round 1, and the 342 × `€` entry at `s3_key_length_limit.rs:316` now closes it;
  - PUT counts characters (`lib.rs:1838`);
  - PUT measures the flat `bucket/key` name;
  - PUT measures the raw wire `key`;
  - the bucket check reads the raw `bucket`, not the decoded `bucket_name` (`lib.rs:1828`);
  - `>` changed to `>=` on PUT;
  - `>` changed to `>=` in bulk delete.

## Attacks on the fix (could not break it)

- **Other spellings that store an over-limit key under some bucket.** I tried:
  - lowercase `%2f` in the bucket segment: `to_digit(16)` decodes it, so it is refused (`lib.rs:3304-3307`);
  - `//b/k`, which resolves to the same bucket and key;
  - `..` segments: the backend stores flat dirent names (directory-entry records) with no path normalization (`crates/server/src/lib.rs:709-723`), and `..` only lengthens the measured key;
  - invalid UTF-8 such as `%FF`: it is decoded lossily to U+FFFD, which is 3 bytes, and the check measures that same `String`, which is exactly what gets stored (`lib.rs:3298-3316`, `:1776-1778`).

  Because `bucket_name` cannot contain `/`, the first `/` of the stored flat key always falls right after the bucket. So the listing in `crates/server/src/lib.rs:714-723` can only show the measured `key_name` as that object's key. I found no alias.
- **Other ways to create an object.** Only the PUT arm calls `put_object_streaming` (`lib.rs:1866`, `:1879`, `:1891`). Multipart, POST-object and CopyObject are refused earlier (`lib.rs:1786`, `:1754`, `:1809`). The length check runs before the payload-mode match (`lib.rs:1838` vs `:1855`), so signed, unsigned and aws-chunked (streaming-signature) uploads are all covered.
- **Gates the frozen CI run skipped.** The run stopped at the advisory check, so the `cargo xtask conformance` step never ran. I ran it on the patched tree: `xtask conformance: 5 valid + 6 invalid vectors pass`. The patch does not touch `Cargo.lock`, `deny.toml` or `deny-all-features.toml`.

## Findings for the human

- NEEDS-HUMAN [human] — **One stored over-limit key blocks bulk cleanup of up to 999 other keys.** `crates/gateway-s3/src/lib.rs:2270` (with `:2114`) refuses a whole DeleteObjects (S3 bulk delete) request if any entry is over 1,024 bytes. Over-limit objects can still exist: those stored before this fix, or written with `wyrd put --key`. `list_container` lists them without filtering (`crates/server/src/lib.rs:714-741`). Concrete case:
  1. Bucket `b` holds `b/<'a' × 1025>` plus 999 ordinary objects.
  2. A list-then-bulk-delete client (boto3 `bucket.objects.all().delete()`, `mc rm --recursive`) sends one 1,000-key DeleteObjects for that listing page.
  3. It gets `400 KeyTooLongError`, none of the 999 ordinary keys is deleted, and the tool stops.

  Nothing is stranded, because single-object DELETE still works (leg 4). But a bucket holding such a key cannot be emptied with standard bulk tools. Criterion (3) of the brief asks for this whole-request refusal, so changing it is a brief-level decision. The alternatives are a per-key `<Error><Code>KeyTooLongError</Code>` row inside a `200` DeleteResult, or no per-entry bound at all (the 8 MiB body cap already bounds memory). This may duplicate the round-1 finding "moot if the human drops the bulk bound" that was held for the human.
- NEEDS-HUMAN [human] — **C4-ci is red for a reason outside this patch.** The `cargo deny --all-features … check advisories` step fails on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (security advisories) in `rustls v0.21.12`, which comes in through `tikv-client v0.4.0` → `wyrd-metadata-tikv` (`gate-logs/C4-ci.log:3480-3557`; the same at `:6484-6561` on retry). Every earlier stage passed: typos, docs lint/render, guards, fmt, clippy, build, the full test run (including all 5 new wire tests and both new unit tests), machete, and the default `cargo deny check`. This is the third round with the same red, and rebuilding this patch cannot change it. It needs a separate dependency bump or a deny exception. This is not evidence against the fix.

## Claims in `check-gates.json` I would not take at face value

- `C4-verify` says "5 test(s) ran red", but only 4 did. Both the frozen log and my run show `1 passed; 4 failed`, and leg 4 is green on both sides by design (brief, Falsifiability). The patch is fine; the harness summary overcounts.
- `T4-batch-review` reports `0 blocking` after 11.87 s. That is fast for three review passes over a ~38 KB diff, and the round before reported 7 blocking at the same results path. `review-batch.md` is not among my inputs, so I can't confirm the review actually re-ran on this diff rather than reusing earlier output. Worth a glance before sign-off.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No actionable findings on either requested lens.

- **Correctness:** No introduced defect identified in the PUT guards (`crates/gateway-s3/src/lib.rs:1828`, `crates/gateway-s3/src/lib.rs:1838`), whole-request bulk refusal (`crates/gateway-s3/src/lib.rs:2110`, `crates/gateway-s3/src/lib.rs:2270`), or regression tests (`crates/server/tests/s3_key_length_limit.rs:240`, `crates/server/tests/s3_key_length_limit.rs:307`, `crates/server/tests/s3_key_length_limit.rs:355`).
- **Reuse, simplification, efficiency:** No actionable cleanup identified. The error response is shared (`crates/gateway-s3/src/lib.rs:2023`), and ordinary test path encoding reuses `sigv4::uri_encode` (`crates/gateway-s3/src/lib.rs:4694`, `crates/server/tests/s3_key_length_limit.rs:205`).

Reviewed the target source and frozen gate evidence; no builds or tests were rerun. The evidence records five passing wire tests, four failures with production reverted, and 11 caught mutants with one unviable. Full CI remains failed on unchanged TiKV/rustls dependency advisories; this report does not override that gate or make an acceptance decision.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide disposition of the unchanged TiKV dependency-policy failure under #543 — independent red→green passes, but both audits reject RUSTSEC-2026-0346/0349, so the full gate remains red; the supplementary DST sweep is incomplete (`reviewer-evidence/green.log:15`; `reviewer-evidence/deny-all-advisories.log:10`, `reviewer-evidence/deny-all-advisories.log:40`; `deny-all-features.toml:80`).
- [ ] Validation — fitness-to-purpose — Accept the intended compatibility tradeoff — a legacy over-limit key now refuses an entire bulk-delete batch, while individual deletion remains available; decide whether that meets operator/client needs (`brief.md:29`; `docs/design/architecture/05-building-block-view.md:137`; `crates/server/tests/s3_key_length_limit.rs:355`).
- [ ] **One stored over-limit key blocks bulk cleanup of up to 999 other keys.** `crates/gateway-s3/src/lib.rs:2270` (with `:2114`) refuses a whole DeleteObjects (S3 bulk delete) request if any entry is over 1,024 bytes. Over-limit objects can still exist: those stored before this fix, or written with `wyrd put --key`. `list_container` lists them without filtering (`crates/server/src/lib.rs:714-741`). Concrete case:
- [ ] **C4-ci is red for a reason outside this patch.** The `cargo deny --all-features … check advisories` step fails on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (security advisories) in `rustls v0.21.12`, which comes in through `tikv-client v0.4.0` → `wyrd-metadata-tikv` (`gate-logs/C4-ci.log:3480-3557`; the same at `:6484-6561` on retry). Every earlier stage passed: typos, docs lint/render, guards, fmt, clippy, build, the full test run (including all 5 new wire tests and both new unit tests), machete, and the default `cargo deny check`. This is the third round with the same red, and rebuilding this patch cannot change it. It needs a separate dependency bump or a deny exception. This is not evidence against the fix.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] external dependency: RustSec advisory DB (RUSTSEC-2026-0346, RUSTSEC-2026-0349 on tikv-client's rustls 0.21.12, published 2026-10-14) — the gating C4-ci row (`cargo xtask ci`) fails at its second `cargo deny` step on the unpatched base `7bf7eb9`, so no bundle on this base can show a green C4-ci. A ready fix is in this bundle as `base-advisory-waiver.diff` (both are medium/low, below the file's high/critical review trigger; no rustls 0.21.x fix exists). Land it as its own PR (as #790 did for RUSTSEC-2026-0258), then re-run this bundle's C4-ci. A full `cargo xtask ci` with patch + waiver applied is recorded under Verification.
- [ ] **The creation invariant has a bucket-separator bypass that the scope excludes.** `brief.md:42-43` promises that no oversized S3 key can be created, but `brief.md:27-28` measures only the decoded request key and `brief.md:69-70` excludes bucket naming. The target splits on a literal slash before decoding (`crates/gateway-s3/src/lib.rs:567-572`), then decodes both components into one flat storage key (`:1765-1769`), without rejecting a decoded slash in the bucket. Source-derived counterexample: a signed `PUT /b%2Fa/<1023 ASCII x characters>` has a 1,023-byte request key, so the proposed check accepts it; it addresses exactly the same stored object as `/b/a/<1023 ASCII x characters>`, whose key under bucket `b` is 1,025 bytes. Bulk deletion through `/b?delete` naming that canonical key would then be refused by criterion (3). This leaves the thread's write-path-first concern reachable through an alias despite all proposed boundary examples passing. Revise the brief to include a narrow separator-alias refusal and a wire regression, or explicitly qualify the invariant and record this bypass as a deferral; the unconditional promise and current exclusion cannot both stand.
- [ ] **Criterion (4) can pass without proving its promised escape path.** `brief.md:33-36` requires only `DELETE` → `204` to establish that legacy or non-S3 oversized objects remain deletable. The target deliberately returns `204` for both present and absent keys (`crates/gateway-s3/src/lib.rs:1887-1889`); the backend distinguishes these as `Ok(true)` and `Ok(false)` (`crates/server/src/lib.rs:650-665`). After criterion (1)'s rejected PUT, deleting that same absent key would satisfy (4) without exercising removal of any oversized object. Require a pre-existing oversized fixture seeded through the existing backend API (`crates/server/src/lib.rs:235`), HEAD `200` before deletion, then signed DELETE `204` and HEAD `404` afterwards. Allow that fixture setup explicitly alongside the wire assertions in `brief.md:39`; it needs no new production API and remains runnable on the reverted base.
- [ ] **The patch strands objects whose keys are too long for a request line.** `crates/gateway-s3/src/lib.rs:2266` now refuses any bulk-delete `<Key>` over 1,024 bytes. The patch relies on single-object DELETE (`crates/gateway-s3/src/lib.rs:1917`) as the way out for *every* over-limit object (the claims at `:493`, `:1818`, `:1978` and `crates/server/tests/s3_key_length_limit.rs:19`). That way out has a ceiling. The gateway runs on `axum::serve` with hyper defaults (`crates/gateway-s3/src/lib.rs:279`). Hyper parses the request target with the `http` crate (`Cargo.lock:1608-1609`, 1.4.2), whose `Uri` rejects anything over 65,534 bytes, and hyper answers `414` before the gateway runs. **Concrete failing case:** seed `b/<'a' × 70,000>` through `Gateway::put_object`, as leg 4 does. On the patched code, HEAD is `414`, DELETE is `414`, bulk delete is `400 KeyTooLongError`, and the object is still present. On the pre-fix base, HEAD and DELETE are the same `414`s, but bulk delete answers `200` and the object is gone. Such keys can exist. `wyrd put --key` (`crates/server/src/cli.rs:562-568`) takes the key from argv (up to 128 KiB per argument on Linux) into the same metadata store. The CLI has no `delete` subcommand (`crates/server/src/cli.rs:415-416`), so S3 bulk delete was the only exit. Keys that need percent-encoding hit the ceiling sooner, at about 21,800 key bytes, because each byte becomes 3 wire bytes. Leg 4 seeds only a 1,025-byte key (`s3_key_length_limit.rs:319`), so it can't see this. Tagged `[human]` because the builder did what the brief said: criterion (3) demands the bulk refusal, while the invariant and C-1 demand that nothing becomes undeletable, and the two conflict above the URI ceiling. Options: drop the per-entry bound from bulk delete and keep the body cap; raise the bound above what a single DELETE can address; or accept the gap and record it (and/or apply the 1,024-byte limit to `wyrd put --key`, which the brief left out of scope).
- [ ] Refusing the whole bulk-delete request (`crates/gateway-s3/src/lib.rs:2270`, `:2114`) makes bucket-emptying tools fail on any batch that contains a legacy over-limit key. Concrete case, run as a probe against the patched tree: I seeded `b/<'a' × 1025>` through `Gateway::put_object` and PUT `k1`, `k2`, `k3`. ListObjectsV2 returns all 4 keys, including the long one. A `DeleteObjects` built from that listing answers `400 KeyTooLongError`, and `k1`, `k2` and `k3` all survive. Any client that lists keys and then deletes them in batches (`aws s3 rm --recursive`, `mc rm -r`, `rclone purge`) fails for every 1,000-key batch holding such a key, not just for that one key. The object itself is not stranded, because single-object `DELETE` still removes it (leg 4), so the brief's invariant holds as written. The patch does what criterion (3) asks. Whether whole-request refusal is the right design, or whether a per-key `<Error><Code>KeyTooLongError</Code>` row (or no bound at all) would be better, is a brief-level call. This probably overlaps the round-1 finding already held for the human ("moot if the human drops the bulk bound").
- [ ] The only failing gating row, C4 `cargo xtask ci`, fails on `cargo deny --all-features ... check advisories`: RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls v0.21.12` (`gate-logs/C4-ci.log:3475-3557`, `Cargo.lock:3294-3295`). The patch touches only `crates/gateway-s3/src/lib.rs`, the new test file and the architecture doc. It changes no `Cargo.toml` or `Cargo.lock`. Every earlier stage in that log passed: fmt, clippy `--all-targets`, build, the full test run including the 3 new unit tests and the 5 wire tests, typos, and docs-check. No rebuild of this patch can clear the row. A dependency bump or a recorded advisory exception has to be decided separately.
- [ ] external dependency: RustSec advisory DB (RUSTSEC-2026-0346, RUSTSEC-2026-0349 on tikv-client's rustls 0.21.12) — the gating C4-ci row (`cargo xtask ci`) fails at its second `cargo deny` step on the unpatched base `7bf7eb9`, so no bundle on this base can show a green C4-ci; a maintainer must waive or otherwise resolve the two advisories in a separate PR against `deny-all-features.toml` (sketch above, subject to the file's #543 review trigger), after which this bundle's C4-ci can be re-run.

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide disposition of the unchanged TiKV dependency-policy failure under #543 — independent red→green passes, but both audits reject RUSTSEC-2026-0346/0349, so the full gate remains red; the supplementary DST sweep is incomplete (`reviewer-evidence/green.log:15`; `reviewer-evidence/deny-all-advisories.log:10`, `reviewer-evidence/deny-all-advisories.log:40`; `deny-all-features.toml:80`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 5 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
