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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 94.7% — 179 of 189 instrumentable changed lines executed (floor 80%); 189 of 324 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 12 mutants tested in 48s: 11 caught, 1 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_620/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.48s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #620: enforce S3's 1,024-byte decoded-key limit without stranding existing objects; no patch defect found, with full-CI disposition and fitness approval still owed.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The five wire criteria distinguish byte length, encoded-path aliases, atomic bulk refusal, and legacy deletion, making the intended compatibility boundary falsifiable (`brief.md:19`; `crates/server/tests/s3_key_length_limit.rs:251`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes while retaining the new test compiled successfully and produced four expected 200-versus-400 failures; legacy deletion remained green (`reviewer-evidence/red.log:268`, `reviewer-evidence/red.log:313`). |
| C3 Change | PASS | All five criteria pass after restoration; changes stay within the gateway, regression tests, and required living documentation (`reviewer-evidence/green.log:15`; `docs/design/architecture/05-building-block-view.md:137`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the existing TiKV advisory-policy failure and obtain a complete CI run — focused red→green passes, but frozen CI rejects unchanged dependencies and the independent run hits a read-only advisory-cache lock (`gate-logs/C4-ci.log:6480`, `gate-logs/C4-ci.log:6510`; `reviewer-evidence/ci.log:3267`; `Cargo.lock:3294`). |
| C5 Causal adequacy | PASS | Admission checks close the actual creation and bucket-alias paths before storage; complete bulk validation precedes deletion, while legacy single DELETE remains available; no optional-capability or load-time symptom guard was added (`crates/gateway-s3/src/lib.rs:1828`, `crates/gateway-s3/src/lib.rs:1838`, `crates/gateway-s3/src/lib.rs:2114`, `crates/gateway-s3/src/lib.rs:1920`). |
| T1 Structure | PASS | The bound belongs at the S3 boundary and uses existing error and gateway seams; no dependency, trait, persisted-format, or clock-lifecycle expansion is required (`crates/gateway-s3/src/lib.rs:497`, `crates/gateway-s3/src/lib.rs:2020`). |
| T2 Shape | PASS | PUT and bulk deletion expose the same S3 error identity, count decoded UTF-8 bytes, and preserve legal boundary inputs and XML entity semantics (`crates/gateway-s3/src/lib.rs:2024`, `crates/gateway-s3/src/lib.rs:2270`, `crates/gateway-s3/src/lib.rs:4542`). |
| T3 Runtime | PASS | Rejected PUTs cannot enter the write seam and rejected batches cannot begin fan-out; existing request buffering remains bounded, with no new production await or helper task (`crates/gateway-s3/src/lib.rs:1850`, `crates/gateway-s3/src/lib.rs:2110`, `crates/gateway-s3/src/lib.rs:2138`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit must run at publish, so the deferred row is neither green nor a missing-evidence finding (`gate-logs/T4-contribution.log:10`); path-based prior-art review is recorded below. |
| T5 Judgment | PASS | Tests now detect multibyte bulk-bound errors, prove legacy object existence before deletion, bound socket/backend work, and abort the owned server on drop; independent mutation testing leaves no viable survivor (`crates/server/tests/s3_key_length_limit.rs:326`, `crates/server/tests/s3_key_length_limit.rs:378`, `crates/server/tests/s3_key_length_limit.rs:83`, `crates/server/tests/s3_key_length_limit.rs:89`; `reviewer-evidence/mutants-capped.log:17`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept whole-batch refusal when a legacy over-limit key is present, with single-object DELETE as the recovery route — execution proves removability, but whether that workflow meets users' cleanup needs remains a sign-off decision (`brief.md:29`; `crates/server/tests/s3_key_length_limit.rs:339`, `crates/server/tests/s3_key_length_limit.rs:378`; `docs/design/architecture/05-building-block-view.md:137`). |

The patch's behavioral evidence is independently reproduced. `cargo test --locked --offline -p wyrd-server --test s3_key_length_limit` failed four tests on the stashed base and passed all five after `git stash pop`. The gateway's 80 tests and the existing 22 bulk-delete plus 24 listing tests also passed (`reviewer-evidence/gateway-tests.log:110`; `reviewer-evidence/peer-wire-tests.log:31`, `reviewer-evidence/peer-wire-tests.log:61`). These runs used the real loopback gateway and redb/filesystem backend, including a backend-seeded legacy object. The target matched the supplied patch before and after testing; source citations above are relative to `$PDCA_TARGET`, and evidence citations are relative to this report's directory.

The unresolved CI result is outside this patch's implementation. The frozen run passed spelling, documentation, formatting, clippy, build, workspace tests, dependency-use checks, and default dependency policy, then rejected RUSTSEC-2026-0346 and RUSTSEC-2026-0349 through TiKV's rustls 0.21.12 (`gate-logs/C4-ci.log:6471`, `gate-logs/C4-ci.log:6486`, `gate-logs/C4-ci.log:6516`). This patch changes no dependency or policy file. The target explicitly requires reviewing new advisories in that retained backend (`deny-all-features.toml:80`; existing backend decision context: [issue #435](https://github.com/getwyrd/wyrd/issues/435)). This is not a request to reopen previously accepted exceptions or to broaden #620. The independent `cargo xtask ci` likewise passed its preceding checks through cargo-machete, then could not acquire the advisory database's lock on this sandbox's read-only cache (`reviewer-evidence/ci.log:3261`, `reviewer-evidence/ci.log:3267`). That local failure is a host caveat, not a patch defect. Neither run establishes a complete green CI result; the later conformance, statics, orchestrator guard, and DST stages remain unrun in these CI executions (`xtask/src/main.rs:1683`).

Additional gate evidence supports the focused result:

- Independent spelling, formatting, docs lint, and docs rendering passed; the brief's external `typos` dependency was actually exercised (`reviewer-evidence/typos.log:4`; `reviewer-evidence/docs-render.log:5`).
- Independent `cargo mutants --cap-lints true --in-diff patch.diff`, scoped to the gateway and its tests, reproduced 11 caught and one unviable mutant (`reviewer-evidence/mutants-capped.log:17`; `gate-logs/C5-mutants.log:13`). The initial uncapped attempt was weakened by denied unused-code warnings; the cited rerun removes that ambiguity without changing the patch.
- The instance-scoped diff-coverage wrapper was adjudicated from its captured log: 179/189 instrumentable changed lines executed (94.7%), with 135 unscored lines explicitly excluded from that claim (`gate-logs/C4-diff-cov.log:133`, `gate-logs/C4-diff-cov.log:156`). This percentage was not independently recalculated.
- The frozen TiKV feature gate shows real clippy compilations for the backend and server selections completing, and the batched review reports zero blocking findings; these are log-supported outcomes, not independent reruns (`gate-logs/host-tikv.log:211`; `gate-logs/T4-batch-review.log:10`).

Prior art was checked by affected path against merged history at base `7bf7eb906c634d57d28f2ae26fd0b412732948d5`, including the gateway, the new test path, and the architecture document. The merged gateway history includes bulk-delete work from [PR #612](https://github.com/getwyrd/wyrd/pull/612); [issue #620](https://github.com/getwyrd/wyrd/issues/620) explicitly carries the deferred key-bound work. All 19 closed, unmerged PRs were enumerated and their changed-file lists checked against all three affected paths; none matched (`reviewer-evidence/merged-path-history.tsv:11`; `reviewer-evidence/other-merged-path-history.log:1`; `reviewer-evidence/closed-pr-path-check.log:1`). Existing unrelated deferrals remain settled. The specified byte bound and error code agree with the primary [AWS object-key guidance](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-keys.html) and [S3 error reference](https://docs.aws.amazon.com/AmazonS3/latest/API/API_Error.html).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 620 (S3 1,024-byte object-key limit)

I tried to refute the evidence, the PUT fix, the bulk-delete bound and the gate verdict. I could not break the PUT side. Two items need a human decision. Neither is a defect the builder can fix by rebuilding this patch.

## Evidence: reproduced and holds up

- `crates/server/tests/s3_key_length_limit.rs:252-398`: I re-ran the red→green proof myself in a scratch copy. With `crates/gateway-s3/src/lib.rs` reverted to base and the test kept, 4 of 5 tests fail (`200` where `400` was expected). Leg 4 passes on both sides, as the brief intends. With the patch, all 5 pass. The test drives a real loopback server through signed raw HTTP, so it exercises the production router. It is not a copy of the production logic.
- `crates/gateway-s3/src/lib.rs:1828`, `:1838`, `:2270`: I tried five plausible wrong fixes, and each one turns the wire test red: measuring `bucket/key` instead of the key, measuring the percent-encoded wire key, counting characters on PUT, counting characters in the bulk-delete parser, and checking the raw (undecoded) bucket for `/`. The test is not a tautology, and the round-1 gap (leg 3 used only ASCII keys) is closed by the `€` entry at `s3_key_length_limit.rs:319`.
- Bypass attempts that failed:
  - `PUT //b/<'z' × 1025>` → `400`.
  - `PUT /b/<'%FF' × 342>` → `400 KeyTooLongError`. Lossy decoding stores 342 × U+FFFD, which is 1,026 bytes, so the size the check measures equals the size that would be stored.
  - The only calls that create objects are the three `put_object_streaming` calls at `lib.rs:1866`, `:1879` and `:1891`, and all three sit after the check.
  - `POST /b%2Fa?delete` still deletes `b/a/<'x' × 1023>`. That is a delete, not a create, so it is consistent with the invariant.

## Findings

- NEEDS-HUMAN [human] — Refusing the whole bulk-delete request (`crates/gateway-s3/src/lib.rs:2270`, `:2114`) makes bucket-emptying tools fail on any batch that contains a legacy over-limit key. Concrete case, run as a probe against the patched tree: I seeded `b/<'a' × 1025>` through `Gateway::put_object` and PUT `k1`, `k2`, `k3`. ListObjectsV2 returns all 4 keys, including the long one. A `DeleteObjects` built from that listing answers `400 KeyTooLongError`, and `k1`, `k2` and `k3` all survive. Any client that lists keys and then deletes them in batches (`aws s3 rm --recursive`, `mc rm -r`, `rclone purge`) fails for every 1,000-key batch holding such a key, not just for that one key. The object itself is not stranded, because single-object `DELETE` still removes it (leg 4), so the brief's invariant holds as written. The patch does what criterion (3) asks. Whether whole-request refusal is the right design, or whether a per-key `<Error><Code>KeyTooLongError</Code>` row (or no bound at all) would be better, is a brief-level call. This probably overlaps the round-1 finding already held for the human ("moot if the human drops the bulk bound").
- NEEDS-HUMAN [human] — The only failing gating row, C4 `cargo xtask ci`, fails on `cargo deny --all-features ... check advisories`: RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls v0.21.12` (`gate-logs/C4-ci.log:3475-3557`, `Cargo.lock:3294-3295`). The patch touches only `crates/gateway-s3/src/lib.rs`, the new test file and the architecture doc. It changes no `Cargo.toml` or `Cargo.lock`. Every earlier stage in that log passed: fmt, clippy `--all-targets`, build, the full test run including the 3 new unit tests and the 5 wire tests, typos, and docs-check. No rebuild of this patch can clear the row. A dependency bump or a recorded advisory exception has to be decided separately.
- Non-blocking wording fix, not tagged: `docs/design/architecture/05-building-block-view.md:137` says "The limit is enforced only where an object is created." Two sentences later the same paragraph says bulk `DeleteObjects` refuses an over-limit key. The code comment at `crates/gateway-s3/src/lib.rs:493` gets this right ("Enforced where an object is CREATED … and per entry in a bulk DeleteObjects body"). Fold the correction into whatever edit the first finding produces. It is not worth a rebuild on its own.

I found no unwarranted claim in `check-gates.json`. C4-verify (4 red, then green), C5 (11 of 12 mutants caught, 1 unviable) and the 94.7% diff coverage all match what I reproduced or what the logs show. The `overall: fail` comes only from the `rustls` advisory row.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No introduced correctness bugs found in this diff. One low-priority reuse/efficiency finding:

- NEEDS-HUMAN [impl] — `crates/gateway-s3/src/lib.rs:4625` and `crates/server/tests/s3_key_length_limit.rs:136`: The new `path_encode` and `encode` helpers both duplicate the existing public `sigv4::uri_encode(key, true)` (`crates/gateway-s3/src/sigv4.rs:163`), including its unreserved set, slash escaping, and uppercase UTF-8 byte escapes. Reuse that helper to remove both copies and avoid their temporary `String` allocation per input byte. Keep `encode_all`, whose deliberate escaping of unreserved bytes exercises a different case. This is an optional test-code cleanup, not a correctness defect.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Resolve the existing TiKV advisory-policy failure and obtain a complete CI run — focused red→green passes, but frozen CI rejects unchanged dependencies and the independent run hits a read-only advisory-cache lock (`gate-logs/C4-ci.log:6480`, `gate-logs/C4-ci.log:6510`; `reviewer-evidence/ci.log:3267`; `Cargo.lock:3294`).
- [ ] Validation — fitness-to-purpose — Accept whole-batch refusal when a legacy over-limit key is present, with single-object DELETE as the recovery route — execution proves removability, but whether that workflow meets users' cleanup needs remains a sign-off decision (`brief.md:29`; `crates/server/tests/s3_key_length_limit.rs:339`, `crates/server/tests/s3_key_length_limit.rs:378`; `docs/design/architecture/05-building-block-view.md:137`).
- [ ] Refusing the whole bulk-delete request (`crates/gateway-s3/src/lib.rs:2270`, `:2114`) makes bucket-emptying tools fail on any batch that contains a legacy over-limit key. Concrete case, run as a probe against the patched tree: I seeded `b/<'a' × 1025>` through `Gateway::put_object` and PUT `k1`, `k2`, `k3`. ListObjectsV2 returns all 4 keys, including the long one. A `DeleteObjects` built from that listing answers `400 KeyTooLongError`, and `k1`, `k2` and `k3` all survive. Any client that lists keys and then deletes them in batches (`aws s3 rm --recursive`, `mc rm -r`, `rclone purge`) fails for every 1,000-key batch holding such a key, not just for that one key. The object itself is not stranded, because single-object `DELETE` still removes it (leg 4), so the brief's invariant holds as written. The patch does what criterion (3) asks. Whether whole-request refusal is the right design, or whether a per-key `<Error><Code>KeyTooLongError</Code>` row (or no bound at all) would be better, is a brief-level call. This probably overlaps the round-1 finding already held for the human ("moot if the human drops the bulk bound").
- [ ] The only failing gating row, C4 `cargo xtask ci`, fails on `cargo deny --all-features ... check advisories`: RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls v0.21.12` (`gate-logs/C4-ci.log:3475-3557`, `Cargo.lock:3294-3295`). The patch touches only `crates/gateway-s3/src/lib.rs`, the new test file and the architecture doc. It changes no `Cargo.toml` or `Cargo.lock`. Every earlier stage in that log passed: fmt, clippy `--all-targets`, build, the full test run including the 3 new unit tests and the 5 wire tests, typos, and docs-check. No rebuild of this patch can clear the row. A dependency bump or a recorded advisory exception has to be decided separately.
- [ ] `crates/gateway-s3/src/lib.rs:4625` and `crates/server/tests/s3_key_length_limit.rs:136`: The new `path_encode` and `encode` helpers both duplicate the existing public `sigv4::uri_encode(key, true)` (`crates/gateway-s3/src/sigv4.rs:163`), including its unreserved set, slash escaping, and uppercase UTF-8 byte escapes. Reuse that helper to remove both copies and avoid their temporary `String` allocation per input byte. Keep `encode_all`, whose deliberate escaping of unreserved bytes exercises a different case. This is an optional test-code cleanup, not a correctness defect.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] external dependency: RustSec advisory DB (RUSTSEC-2026-0346, RUSTSEC-2026-0349 on tikv-client's rustls 0.21.12) — the gating C4-ci row (`cargo xtask ci`) fails at its second `cargo deny` step on the unpatched base `7bf7eb9`, so no bundle on this base can show a green C4-ci; a maintainer must waive or otherwise resolve the two advisories in a separate PR against `deny-all-features.toml` (sketch above, subject to the file's #543 review trigger), after which this bundle's C4-ci can be re-run.
- [ ] **The creation invariant has a bucket-separator bypass that the scope excludes.** `brief.md:42-43` promises that no oversized S3 key can be created, but `brief.md:27-28` measures only the decoded request key and `brief.md:69-70` excludes bucket naming. The target splits on a literal slash before decoding (`crates/gateway-s3/src/lib.rs:567-572`), then decodes both components into one flat storage key (`:1765-1769`), without rejecting a decoded slash in the bucket. Source-derived counterexample: a signed `PUT /b%2Fa/<1023 ASCII x characters>` has a 1,023-byte request key, so the proposed check accepts it; it addresses exactly the same stored object as `/b/a/<1023 ASCII x characters>`, whose key under bucket `b` is 1,025 bytes. Bulk deletion through `/b?delete` naming that canonical key would then be refused by criterion (3). This leaves the thread's write-path-first concern reachable through an alias despite all proposed boundary examples passing. Revise the brief to include a narrow separator-alias refusal and a wire regression, or explicitly qualify the invariant and record this bypass as a deferral; the unconditional promise and current exclusion cannot both stand.
- [ ] **Criterion (4) can pass without proving its promised escape path.** `brief.md:33-36` requires only `DELETE` → `204` to establish that legacy or non-S3 oversized objects remain deletable. The target deliberately returns `204` for both present and absent keys (`crates/gateway-s3/src/lib.rs:1887-1889`); the backend distinguishes these as `Ok(true)` and `Ok(false)` (`crates/server/src/lib.rs:650-665`). After criterion (1)'s rejected PUT, deleting that same absent key would satisfy (4) without exercising removal of any oversized object. Require a pre-existing oversized fixture seeded through the existing backend API (`crates/server/src/lib.rs:235`), HEAD `200` before deletion, then signed DELETE `204` and HEAD `404` afterwards. Allow that fixture setup explicitly alongside the wire assertions in `brief.md:39`; it needs no new production API and remains runnable on the reverted base.
- [ ] **The patch strands objects whose keys are too long for a request line.** `crates/gateway-s3/src/lib.rs:2266` now refuses any bulk-delete `<Key>` over 1,024 bytes. The patch relies on single-object DELETE (`crates/gateway-s3/src/lib.rs:1917`) as the way out for *every* over-limit object (the claims at `:493`, `:1818`, `:1978` and `crates/server/tests/s3_key_length_limit.rs:19`). That way out has a ceiling. The gateway runs on `axum::serve` with hyper defaults (`crates/gateway-s3/src/lib.rs:279`). Hyper parses the request target with the `http` crate (`Cargo.lock:1608-1609`, 1.4.2), whose `Uri` rejects anything over 65,534 bytes, and hyper answers `414` before the gateway runs. **Concrete failing case:** seed `b/<'a' × 70,000>` through `Gateway::put_object`, as leg 4 does. On the patched code, HEAD is `414`, DELETE is `414`, bulk delete is `400 KeyTooLongError`, and the object is still present. On the pre-fix base, HEAD and DELETE are the same `414`s, but bulk delete answers `200` and the object is gone. Such keys can exist. `wyrd put --key` (`crates/server/src/cli.rs:562-568`) takes the key from argv (up to 128 KiB per argument on Linux) into the same metadata store. The CLI has no `delete` subcommand (`crates/server/src/cli.rs:415-416`), so S3 bulk delete was the only exit. Keys that need percent-encoding hit the ceiling sooner, at about 21,800 key bytes, because each byte becomes 3 wire bytes. Leg 4 seeds only a 1,025-byte key (`s3_key_length_limit.rs:319`), so it can't see this. Tagged `[human]` because the builder did what the brief said: criterion (3) demands the bulk refusal, while the invariant and C-1 demand that nothing becomes undeletable, and the two conflict above the URI ceiling. Options: drop the per-entry bound from bulk delete and keep the body cap; raise the bound above what a single DELETE can address; or accept the gap and record it (and/or apply the 1,024-byte limit to `wyrd put --key`, which the brief left out of scope).

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Resolve the existing TiKV advisory-policy failure and obtain a complete CI run — focused red→green passes, but frozen CI rejects unchanged dependencies and the independent run hits a read-only advisory-cache lock (`gate-logs/C4-ci.log:6480`, `gate-logs/C4-ci.log:6510`; `reviewer-evidence/ci.log:3267`; `Cargo.lock:3294`).; `crates/gateway-s3/src/lib.rs:4625` and `crates/server/tests/s3_key_length_limit.rs:136`: The new `path_encode` and `encode` helpers both duplicate the existing public `sigv4::uri_encode(key, true)` (`crates/gateway-s3/src/sigv4.rs:163`), including its unreserved set, slash escaping, and uppercase UTF-8 byte escapes. Reuse that helper to remove both copies and avoid their temporary `String` allocation per input byte. Keep `encode_all`, whose deliberate escaping of unreserved bytes exercises a different case. This is an optional test-code cleanup, not a correctness defect.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 5 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
