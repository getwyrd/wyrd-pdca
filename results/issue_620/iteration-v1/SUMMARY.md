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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 22 of 22 instrumentable changed lines executed (floor 80%); 22 of 65 changed lines were instrumen
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 12 mutants tested in 44s: 8 missed, 3 caught, 1 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_620/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.64s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #620: enforce S3’s 1,024-byte decoded-key limit without stranding existing objects; the functional fix passes, while documentation and test-harness conventions need correction.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The five wire-level criteria distinguish byte count, path aliases, atomic bulk refusal, and legacy deletion, with explicit exclusions and a satisfied `typos` dependency (`brief.md:17`, `brief.md:80`, `brief.md:94`; `reviewer-evidence/scanners.log:3`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing only production changes leaves the new test compiling on the base: four expected assertions fail with HTTP 200 instead of 400, while legacy deletion passes (`reviewer-evidence/pinned-red.log:311`; `crates/server/tests/s3_key_length_limit.rs:194`). |
| C3 Change | PASS | Creation and bulk deletion obey the same decoded-byte boundary before storage/deletion effects, while existing over-limit objects retain single-DELETE access (`crates/gateway-s3/src/lib.rs:1825`, `crates/gateway-s3/src/lib.rs:1835`, `crates/gateway-s3/src/lib.rs:2110`, `crates/gateway-s3/src/lib.rs:2266`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the unchanged TiKV dependency-policy failure and complete the halted CI stages — independent red→green and 27 wire tests pass, but the frozen full gate stops on two rustls advisories outside this patch (`reviewer-evidence/pinned-green.log:30`, `reviewer-evidence/pinned-green.log:41`; `gate-logs/C4-ci.log:3473`; `Cargo.lock:3294`). |
| C5 Causal adequacy | PASS | The missing input boundary is corrected at the creation/parser boundaries, with no capability probe or load-time workaround; explicit wire-suite mutation testing catches all 11 viable mutations, including every frozen survivor (`crates/gateway-s3/src/lib.rs:1835`, `crates/gateway-s3/src/lib.rs:2266`; `reviewer-evidence/mutants-capped.log:15`). |
| T1 Structure | PASS | Existing gateway trait seams and parse-before-delete ordering remain intact; no new production clock, global state, crate, or backend dependency is introduced (`crates/gateway-s3/src/lib.rs:2048`, `crates/gateway-s3/src/lib.rs:2106`, `crates/gateway-s3/src/lib.rs:2134`). |
| T2 Shape | PASS | Shared limit/error handling stays within the planned surface; independent formatting, spelling, documentation lint, and whitespace checks pass (`crates/gateway-s3/src/lib.rs:494`, `crates/gateway-s3/src/lib.rs:2020`; `reviewer-evidence/scanners.log:1`). |
| T3 Runtime | FAIL | The new wire helper can wait indefinitely for a stalled socket, and Harness does not own cancellation of its server task, contrary to the explicit await/lifetime convention (`crates/server/tests/s3_key_length_limit.rs:153`, `crates/server/tests/s3_key_length_limit.rs:161`, `crates/server/tests/s3_key_length_limit.rs:92`; `AGENTS.md:181`). |
| T4 Contribution | FAIL | The changed public PUT/DeleteObjects rejection behavior lacks the required same-PR living architecture update; contribution-message audit is separately N/A until publish, and prior-art checking by both affected paths completed (`AGENTS.md:154`; `patch.diff:1`, `patch.diff:147`; `gate-logs/T4-contribution.log:10`; `reviewer-evidence/prior-art.txt:1`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Make the regression harness bounded and cancel its owned server on drop — a stalled response must fail promptly, and fixture teardown must end its helper’s lifetime (`crates/server/tests/s3_key_length_limit.rs:61`, `crates/server/tests/s3_key_length_limit.rs:92`, `crates/server/tests/s3_key_length_limit.rs:153`; `AGENTS.md:181`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the client/workflow impact of whole-batch refusal for over-limit keys and single-object DELETE as the legacy cleanup route — executable tests prove these mechanics, while operational fitness remains a sign-off decision (`brief.md:26`, `brief.md:31`; `crates/server/tests/s3_key_length_limit.rs:261`, `crates/server/tests/s3_key_length_limit.rs:317`). |

The three grounded convention findings are builder-fixable. They are the three unique concerns in the seven-entry frozen batch review (`gate-logs/T4-batch-review.log:10`):

1. **Update the living architecture documentation.** PUT now returns `InvalidBucketName` or `KeyTooLongError`, and an over-limit DeleteObjects entry rejects the whole batch. Those client-visible changes trigger `AGENTS.md:154`; the patch changes only the gateway source and its new test. Document the decoded UTF-8 byte boundary, the all-or-nothing rejection, and continued single-object deletion of legacy keys in `docs/design/architecture/` (`crates/gateway-s3/src/lib.rs:1825`, `crates/gateway-s3/src/lib.rs:2110`).
2. **Bound the complete wire exchange.** `signed` connects, writes, flushes, and reads to EOF without a deadline. A regression that keeps a connection open can hang the suite instead of yielding a finite failure. Apply an overall timeout around the exchange with useful request context (`crates/server/tests/s3_key_length_limit.rs:153`, `crates/server/tests/s3_key_length_limit.rs:161`; `AGENTS.md:181`).
3. **Tie the helper task to Harness.** `tokio::spawn` discards its JoinHandle, and Harness holds only the address, backend, and temporary directory. Runtime shutdown eventually cancels the task in these tests, but dropping Harness itself does not; retain the task handle and abort it during Harness teardown before its temporary storage is dropped (`crates/server/tests/s3_key_length_limit.rs:61`, `crates/server/tests/s3_key_length_limit.rs:92`; `AGENTS.md:182`).

The behavior is independently reproduced on the supplied disposable target. Source citations above resolve under `$PDCA_TARGET`; its base matches the brief’s `7bf7eb9`, and both restored file hashes match `patch.diff`’s post-images. Production changes were stashed with the new test retained, then restored. With the pinned Rust 1.96.0 toolchain, `cargo test --offline --locked -p wyrd-server --test s3_key_length_limit` produced four expected failures before restoration and five passes afterward. Adding `--test s3_delete_objects` passed all 22 existing tests (`reviewer-evidence/pinned-red.log:311`; `reviewer-evidence/pinned-green.log:30`, `reviewer-evidence/pinned-green.log:41`). No source fix was made during review.

The frozen mutation failures do not establish a remaining causal test gap. Independently running `cargo mutants --in-diff` with `wyrd-server --test s3_key_length_limit` explicitly selected and `--cap-lints=true` caught all eight listed survivors: total 11 caught, one unviable, zero missed (`reviewer-evidence/mutants-capped.log:3`). The unviable mutant requires an unimplemented `DeleteRequest: Default`. Mutation testing used the available Rust 1.96.1 toolchain; the separate red→green confirmation used the repository’s pinned 1.96.0. The frozen coverage log additionally records 22/22 instrumentable changed lines covered; its other 43 changed lines are unscored, not implicitly covered (`gate-logs/C4-diff-cov.log:155`).

The full CI caveat is baseline dependency debt, not a key-limit defect. The frozen log shows formatting, clippy, build, workspace tests, machete, and the default dependency audit completing, followed by failures for `RUSTSEC-2026-0346` and `RUSTSEC-2026-0349` through TiKV’s rustls 0.21.12 (`gate-logs/C4-ci.log:3465`, `gate-logs/C4-ci.log:3474`, `gate-logs/C4-ci.log:3504`). `Cargo.lock` is byte-identical to the base. Conformance, statics, and DST follow that audit and were therefore not reached (`xtask/src/main.rs:1683`). The existing exposure-policy context is [issue #543](https://github.com/getwyrd/wyrd/issues/543), recorded at `deny-all-features.toml:50`; deciding how the newly reported advisory IDs are handled belongs to that dependency-policy work. A local offline audit attempt could not acquire the read-only advisory-database lock (`reviewer-evidence/deny.log:1`), so this gate’s adjudication rests on its readable frozen log. That local host limitation is not a patch failure. The separate TiKV feature log shows both real clippy compilations succeeding, without claiming service-level integration coverage (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`).

Contribution artifacts are **N/A, deferred by design**: `pr-description.md` is drafted after Check, and the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No human clearance is requested for that deferral. The prior-art check queried merged history by both exact affected paths at the brief’s base, confirmed [merged PR #612](https://github.com/getwyrd/wyrd/pull/612), and inspected every changed-file list for all 19 closed, unmerged PRs; none touched either affected path (`reviewer-evidence/prior-art.txt:1`). The specification’s byte limit and error mapping agree with [AWS object-key naming](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-keys.html) and [AWS’s KeyTooLongError definition](https://docs.aws.amazon.com/AmazonS3/latest/API/API_Error.html). No additional `INTEGRATION.md` human-only checklist exists in the supplied target.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 620 (s3-object-key-length-limit)

The PUT-side fix held against every input I tried. I found one concrete regression: the new
bulk-delete bound removes the only S3 way to delete an object whose key is too long to fit in a
single-object DELETE request. That object is now stranded, which breaks the brief's own invariant.
All runs below used a scratch copy of `$PDCA_TARGET`, with the patch's own test harness.

- NEEDS-HUMAN [human] — **The patch strands objects whose keys are too long for a request line.**
  `crates/gateway-s3/src/lib.rs:2266` now refuses any bulk-delete `<Key>` over 1,024 bytes. The
  patch relies on single-object DELETE (`crates/gateway-s3/src/lib.rs:1917`) as the way out for
  *every* over-limit object (the claims at `:493`, `:1818`, `:1978` and
  `crates/server/tests/s3_key_length_limit.rs:19`). That way out has a ceiling. The gateway runs on
  `axum::serve` with hyper defaults (`crates/gateway-s3/src/lib.rs:279`). Hyper parses the request
  target with the `http` crate (`Cargo.lock:1608-1609`, 1.4.2), whose `Uri` rejects anything over
  65,534 bytes, and hyper answers `414` before the gateway runs. **Concrete failing case:** seed
  `b/<'a' × 70,000>` through `Gateway::put_object`, as leg 4 does. On the patched code, HEAD is `414`,
  DELETE is `414`, bulk delete is `400 KeyTooLongError`, and the object is still present. On the
  pre-fix base, HEAD and DELETE are the same `414`s, but bulk delete answers `200` and the object is
  gone. Such keys can exist. `wyrd put --key` (`crates/server/src/cli.rs:562-568`) takes the key
  from argv (up to 128 KiB per argument on Linux) into the same metadata store. The CLI has no
  `delete` subcommand (`crates/server/src/cli.rs:415-416`), so S3 bulk delete was the only exit. Keys
  that need percent-encoding hit the ceiling sooner, at about 21,800 key bytes, because each byte
  becomes 3 wire bytes. Leg 4 seeds only a 1,025-byte key (`s3_key_length_limit.rs:319`), so it
  can't see this. Tagged `[human]` because the builder did what the brief said: criterion (3)
  demands the bulk refusal, while the invariant and C-1 demand that nothing becomes undeletable,
  and the two conflict above the URI ceiling. Options: drop the per-entry bound from bulk delete and
  keep the body cap; raise the bound above what a single DELETE can address; or accept the gap and
  record it (and/or apply the 1,024-byte limit to `wyrd put --key`, which the brief left out of
  scope).

- NEEDS-HUMAN [impl] — **The bulk-delete leg doesn't check that the parser counts bytes rather than
  characters.** Leg 3 uses only ASCII keys (`crates/server/tests/s3_key_length_limit.rs:266`,
  `:290`). I changed `crates/gateway-s3/src/lib.rs:2266` from `key.len()` to
  `key.chars().count()`, and all 5 tests still pass. Under that change, a 342 × `€` entry
  (1,026 bytes) would be accepted. The current code does refuse it: I probed it, and it returns
  `400 KeyTooLongError` with the ordinary key untouched. Fix: add a multi-byte over-limit entry to
  leg 3. This is moot if the human drops the bulk bound per the finding above.

- **The C5-mutants red is a scoping artifact, not a weak test.** The mutants run executed only
  `wyrd-gateway-s3`'s own tests (about 1 s per mutant). The new test lives in `crates/server/tests`.
  I applied all five "missed" boundary mutants by hand: `:1835` `>=` and `==`, and `:2266` `>=`,
  `==` and `<`. The new test kills every one. It also kills these: measuring the raw wire key,
  measuring `bucket/key`, counting characters on the PUT side, and dropping the `/`-in-bucket check
  (`:1825`). `key_too_long → Default::default()` returns `200`, which every leg's `400` assertion
  catches. The remaining two (`delete_objects → Default`, the deleted `"Object"` arm) are in
  pre-existing code covered by `s3_delete_objects.rs`.

- **The C4-ci red is not this patch.** The failure is `RUSTSEC-2026-0346` / `RUSTSEC-2026-0349`
  on `rustls 0.21.12`, pulled in by `tikv-client` (`Cargo.lock:302`), in the `--all-features`
  `cargo deny` advisories step. The patch touches no dependency. In that same run every test passed,
  including the new file and the 22 `s3_delete_objects` tests.

- **The red→green evidence holds.** I re-ran the new test and got 5/5 green. The C4-verify log
  shows legs 1, 2, 3 and 5 failing on the base for the right reason (`200` where `400` was
  expected), and leg 4 green both ways, as the brief intends. The test goes through the real
  path: a loopback listener, SigV4, `dispatch`, and the redb + fs backend. It does not use a copy
  of production code.

- **Things I tried that did not break the fix:**
  - A lowercase `%2f` in the bucket is refused with `InvalidBucketName`.
  - `%FF` × 342 is lossily decoded to 1,026 bytes and refused. The string measured at `:1835` is
    the exact string stored at `:1775`, so measurement and storage can't disagree.
  - `//b/<key>` splits to bucket `b`, so there's no alias.
  - The backend key is the flat `dirent:<root>/<bucket>/<key>` (`crates/core/src/metadata.rs:39`)
    with no normalization, so the `%2F` bucket was the only alias to close.
  - A bulk key of 1,024 × `&amp;` is accepted, because the parser measures after entity decoding.
  - A PUT with `Expect: 100-continue` and a declared 1 MiB body, with the body never sent, gets
    `400 KeyTooLongError` within 5 s. The refusal really does come before the body is read.
  - Both refusals sit after SigV4 verification, so an unauthenticated caller learns nothing.

- **About the T4 review's 7 blocking findings (for the human weighing that gate).** Four are about
  the test harness: no socket timeout (`s3_key_length_limit.rs:161-165`) and a discarded server
  `JoinHandle` (`:95`). Every peer suite the brief told the builder to mirror does the same
  (`s3_delete_objects.rs:96,204`, `s3_list_objects.rs:113,206`,
  `s3_range_conditional.rs:176,228`). Fixing it in one file only would split the style; a
  repo-wide issue fits better. The other three are docs-currency findings. `docs/design/architecture/`
  never documents per-operation S3 errors: it doesn't mention DeleteObjects, `MalformedXML`, or
  CopyObject's `501`. So there is no living section to update, and whether a new `400` counts as
  "altering an API operation" is a judgment call.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/tests/s3_key_length_limit.rs:161`: The new HTTP helper awaits connect, writes, and `read_to_end` without a deadline. A server regression that stalls a response or leaves the connection open hangs the test instead of producing a bounded failure. Wrap the complete socket exchange in a timeout, as required by the await-discipline rubric.
- NEEDS-HUMAN [impl] — `crates/server/tests/s3_key_length_limit.rs:92`: The spawned server's `JoinHandle` is discarded, so dropping `Harness` removes its temporary directory without cancelling the server that still owns the backend. Runtime teardown eventually cancels it, but the new harness does not satisfy the explicit abort-on-drop rule. Retain the handle in `Harness` and abort it on drop, including assertion-failure paths.
- NEEDS-HUMAN [impl] — `crates/gateway-s3/src/lib.rs:1835` and `crates/gateway-s3/src/lib.rs:2110`: PUT and DeleteObjects gain new rejection behavior, but the diff contains no living architecture documentation update. Document the decoded-byte limit, PUT's bucket-separator rejection, whole-request bulk refusal, and continued single-object access/deletion of legacy keys, as required by the docs-currency rubric.

No additional production correctness or reuse/simplification/efficiency findings. Review used the target source and frozen gate evidence; no builds were rerun. The five wire tests pass with the fix, and four fail with production reverted. The mutation log does not establish that those wire tests were included, so its survivors alone do not demonstrate weak assertions in this diff.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Resolve the unchanged TiKV dependency-policy failure and complete the halted CI stages — independent red→green and 27 wire tests pass, but the frozen full gate stops on two rustls advisories outside this patch (`reviewer-evidence/pinned-green.log:30`, `reviewer-evidence/pinned-green.log:41`; `gate-logs/C4-ci.log:3473`; `Cargo.lock:3294`).
- [ ] T5 Judgment — Make the regression harness bounded and cancel its owned server on drop — a stalled response must fail promptly, and fixture teardown must end its helper’s lifetime (`crates/server/tests/s3_key_length_limit.rs:61`, `crates/server/tests/s3_key_length_limit.rs:92`, `crates/server/tests/s3_key_length_limit.rs:153`; `AGENTS.md:181`).
- [ ] Validation — fitness-to-purpose — Accept the client/workflow impact of whole-batch refusal for over-limit keys and single-object DELETE as the legacy cleanup route — executable tests prove these mechanics, while operational fitness remains a sign-off decision (`brief.md:26`, `brief.md:31`; `crates/server/tests/s3_key_length_limit.rs:261`, `crates/server/tests/s3_key_length_limit.rs:317`).
- [ ] **The patch strands objects whose keys are too long for a request line.** `crates/gateway-s3/src/lib.rs:2266` now refuses any bulk-delete `<Key>` over 1,024 bytes. The patch relies on single-object DELETE (`crates/gateway-s3/src/lib.rs:1917`) as the way out for *every* over-limit object (the claims at `:493`, `:1818`, `:1978` and `crates/server/tests/s3_key_length_limit.rs:19`). That way out has a ceiling. The gateway runs on `axum::serve` with hyper defaults (`crates/gateway-s3/src/lib.rs:279`). Hyper parses the request target with the `http` crate (`Cargo.lock:1608-1609`, 1.4.2), whose `Uri` rejects anything over 65,534 bytes, and hyper answers `414` before the gateway runs. **Concrete failing case:** seed `b/<'a' × 70,000>` through `Gateway::put_object`, as leg 4 does. On the patched code, HEAD is `414`, DELETE is `414`, bulk delete is `400 KeyTooLongError`, and the object is still present. On the pre-fix base, HEAD and DELETE are the same `414`s, but bulk delete answers `200` and the object is gone. Such keys can exist. `wyrd put --key` (`crates/server/src/cli.rs:562-568`) takes the key from argv (up to 128 KiB per argument on Linux) into the same metadata store. The CLI has no `delete` subcommand (`crates/server/src/cli.rs:415-416`), so S3 bulk delete was the only exit. Keys that need percent-encoding hit the ceiling sooner, at about 21,800 key bytes, because each byte becomes 3 wire bytes. Leg 4 seeds only a 1,025-byte key (`s3_key_length_limit.rs:319`), so it can't see this. Tagged `[human]` because the builder did what the brief said: criterion (3) demands the bulk refusal, while the invariant and C-1 demand that nothing becomes undeletable, and the two conflict above the URI ceiling. Options: drop the per-entry bound from bulk delete and keep the body cap; raise the bound above what a single DELETE can address; or accept the gap and record it (and/or apply the 1,024-byte limit to `wyrd put --key`, which the brief left out of scope).
- [ ] **The bulk-delete leg doesn't check that the parser counts bytes rather than characters.** Leg 3 uses only ASCII keys (`crates/server/tests/s3_key_length_limit.rs:266`, `:290`). I changed `crates/gateway-s3/src/lib.rs:2266` from `key.len()` to `key.chars().count()`, and all 5 tests still pass. Under that change, a 342 × `€` entry (1,026 bytes) would be accepted. The current code does refuse it: I probed it, and it returns `400 KeyTooLongError` with the ordinary key untouched. Fix: add a multi-byte over-limit entry to leg 3. This is moot if the human drops the bulk bound per the finding above.
- [ ] `crates/server/tests/s3_key_length_limit.rs:161`: The new HTTP helper awaits connect, writes, and `read_to_end` without a deadline. A server regression that stalls a response or leaves the connection open hangs the test instead of producing a bounded failure. Wrap the complete socket exchange in a timeout, as required by the await-discipline rubric.
- [ ] `crates/server/tests/s3_key_length_limit.rs:92`: The spawned server's `JoinHandle` is discarded, so dropping `Harness` removes its temporary directory without cancelling the server that still owns the backend. Runtime teardown eventually cancels it, but the new harness does not satisfy the explicit abort-on-drop rule. Retain the handle in `Harness` and abort it on drop, including assertion-failure paths.
- [ ] `crates/gateway-s3/src/lib.rs:1835` and `crates/gateway-s3/src/lib.rs:2110`: PUT and DeleteObjects gain new rejection behavior, but the diff contains no living architecture documentation update. Document the decoded-byte limit, PUT's bucket-separator rejection, whole-request bulk refusal, and continued single-object access/deletion of legacy keys, as required by the docs-currency rubric.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_620/review-b
- [ ] **The creation invariant has a bucket-separator bypass that the scope excludes.** `brief.md:42-43` promises that no oversized S3 key can be created, but `brief.md:27-28` measures only the decoded request key and `brief.md:69-70` excludes bucket naming. The target splits on a literal slash before decoding (`crates/gateway-s3/src/lib.rs:567-572`), then decodes both components into one flat storage key (`:1765-1769`), without rejecting a decoded slash in the bucket. Source-derived counterexample: a signed `PUT /b%2Fa/<1023 ASCII x characters>` has a 1,023-byte request key, so the proposed check accepts it; it addresses exactly the same stored object as `/b/a/<1023 ASCII x characters>`, whose key under bucket `b` is 1,025 bytes. Bulk deletion through `/b?delete` naming that canonical key would then be refused by criterion (3). This leaves the thread's write-path-first concern reachable through an alias despite all proposed boundary examples passing. Revise the brief to include a narrow separator-alias refusal and a wire regression, or explicitly qualify the invariant and record this bypass as a deferral; the unconditional promise and current exclusion cannot both stand.
- [ ] **Criterion (4) can pass without proving its promised escape path.** `brief.md:33-36` requires only `DELETE` → `204` to establish that legacy or non-S3 oversized objects remain deletable. The target deliberately returns `204` for both present and absent keys (`crates/gateway-s3/src/lib.rs:1887-1889`); the backend distinguishes these as `Ok(true)` and `Ok(false)` (`crates/server/src/lib.rs:650-665`). After criterion (1)'s rejected PUT, deleting that same absent key would satisfy (4) without exercising removal of any oversized object. Require a pre-existing oversized fixture seeded through the existing backend API (`crates/server/src/lib.rs:235`), HEAD `200` before deletion, then signed DELETE `204` and HEAD `404` afterwards. Allow that fixture setup explicitly alongside the wire assertions in `brief.md:39`; it needs no new production API and remains runnable on the reverted base.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Resolve the unchanged TiKV dependency-policy failure and complete the halted CI stages — independent red→green and 27 wire tests pass, but the frozen full gate stops on two rustls advisories outside this patch (`reviewer-evidence/pinned-green.log:30`, `reviewer-evidence/pinned-green.log:41`; `gate-logs/C4-ci.log:3473`; `Cargo.lock:3294`).; T5 Judgment — Make the regression harness bounded and cancel its owned server on drop — a stalled response must fail promptly, and fixture teardown must end its helper’s lifetime (`crates/server/tests/s3_key_length_limit.rs:61`, `crates/server/tests/s3_key_length_limit.rs:92`, `crates/server/tests/s3_key_length_limit.rs:153`; `AGENTS.md:181`).; **The bulk-delete leg doesn't check that the parser counts bytes rather than characters.** Leg 3 uses only ASCII keys (`crates/server/tests/s3_key_length_limit.rs:266`, `:290`). I changed `crates/gateway-s3/src/lib.rs:2266` from `key.len()` to `key.chars().count()`, and all 5 tests still pass. Under that change, a 342 × `€` entry (1,026 bytes) would be accepted. The current code does refuse it: I probed it, and it returns `400 KeyTooLongError` with the ordinary key untouched. Fix: add a multi-byte over-limit entry to leg 3. This is moot if the human drops the bulk bound per the finding above.; `crates/server/tests/s3_key_length_limit.rs:161`: The new HTTP helper awaits connect, writes, and `read_to_end` without a deadline. A server regression that stalls a response or leaves the connection open hangs the test instead of producing a bounded failure. Wrap the complete socket exchange in a timeout, as required by the await-discipline rubric.; `crates/server/tests/s3_key_length_limit.rs:92`: The spawned server's `JoinHandle` is discarded, so dropping `Harness` removes its temporary directory without cancelling the server that still owns the backend. Runtime teardown eventually cancels it, but the new harness does not satisfy the explicit abort-on-drop rule. Retain the handle in `Harness` and abort it on drop, including assertion-failure paths.; `crates/gateway-s3/src/lib.rs:1835` and `crates/gateway-s3/src/lib.rs:2110`: PUT and DeleteObjects gain new rejection behavior, but the diff contains no living architecture documentation update. Document the decoded-byte limit, PUT's bucket-separator rejection, whole-request bulk refusal, and continued single-object access/deletion of legacy keys, as required by the docs-currency rubric.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_620/review-b. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
