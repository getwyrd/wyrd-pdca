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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.5% — 294 of 308 instrumentable changed lines executed (floor 80%); 308 of 487 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 5 mutants tested in 25s: 5 caught

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_620/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.60s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The S3 key-limit fix rejects oversized object creation while preserving legacy deletion; no implementation defect was found, with prior-art confirmation and fitness-to-purpose remaining for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The creation-only contract defines observable byte-boundary, alias, and legacy-deletion outcomes, with existing request limits and exclusions explicit (brief.md:24; brief.md:72; brief.md:115). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the new wire test reproduced four HTTP-200-versus-400 failures; all three deletion tests remained green (reviewer-evidence/red.log:270; reviewer-evidence/red.log:316). |
| C3 Change | PASS | Supported PUT forms now reject names outside the specified S3 boundary before storage, while existing read/delete behavior remains available (crates/gateway-s3/src/lib.rs:1798; crates/gateway-s3/src/lib.rs:1853; crates/gateway-s3/src/lib.rs:1864). |
| C4 Verification (red→green) | PASS | Restoring the patch independently passed all seven wire tests, 84 gateway tests, and 22 existing bulk-delete tests; frozen full CI also passed (reviewer-evidence/green.log:17; reviewer-evidence/gateway-tests.log:114; reviewer-evidence/delete-tests.log:31; gate-logs/C4-ci.log:4235). |
| C5 Causal adequacy | PASS | Missing validation at the creating boundary explains the reproduced failures; decoded-key measurement and bucket-separator rejection close both paths across payload forms, without capability probes or delete restrictions (crates/gateway-s3/src/lib.rs:1853; crates/gateway-s3/src/lib.rs:1864; crates/gateway-s3/src/lib.rs:1924). |
| T1 Structure | PASS | The S3 policy stays at the protocol boundary and preserves the existing gateway trait seams; no new backend coupling, mutable global, or destructive production path is introduced (crates/gateway-s3/src/lib.rs:1853; crates/gateway-s3/src/lib.rs:1935). |
| T2 Shape | PASS | Error responses follow the existing gateway convention, and the living architecture states the creation/deletion distinction and residual cap consistently with the code (crates/gateway-s3/src/lib.rs:1865; crates/gateway-s3/src/lib.rs:518; docs/design/architecture/05-building-block-view.md:139). |
| T3 Runtime | PASS | Invalid creation exits before body-stream construction or backend work; bounded exchanges and abort-on-drop contain the new test server, and real storage verifies deletion through the full body cap (crates/gateway-s3/src/lib.rs:1919; crates/server/tests/s3_key_length_limit.rs:118; crates/server/tests/s3_key_length_limit.rs:127; crates/server/tests/s3_key_length_limit.rs:611). |
| T4 Contribution | NEEDS-HUMAN | Confirm merged and closed/rejected prior art by all three affected file paths — the brief records a source-path search, but the supplied target has one synthetic base commit and no remote, so that history cannot be independently substantiated here (brief.md:155; reviewer-evidence/target-state.log:2; reviewer-evidence/target-state.log:5). |
| T5 Judgment | PASS | Earlier regression gaps are addressed with actual deletion of an 8,388,563-byte key and accepted/refused controls for both chunked and unsigned PUTs; no further implementation concern or scope expansion was found (crates/server/tests/s3_key_length_limit.rs:611; crates/server/tests/s3_key_length_limit.rs:708). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the creation-only S3 boundary for intended clients and legacy cleanup — the wire behavior is demonstrated, but native-API keys exceeding the existing bulk-body cap still have no S3 recovery route, as explicitly excluded from this fix (brief.md:81; docs/design/architecture/05-building-block-view.md:139). |

All source citations above resolve in the supplied `$PDCA_TARGET`; artifact and evidence paths resolve in this review directory. The target matched the supplied patch: reverse-apply checking and whitespace checking succeeded after restoration, and the original three-file change was preserved (reviewer-evidence/target-state.log:10). There was no stale-target caveat.

Independent execution supports the patch's behavior:

- **Actual red→green:** `git stash push` removed the tracked fix while leaving the added integration test available. The pre-fix tree compiled and failed on oversized ASCII, multibyte UTF-8, bucket alias, and chunked PUT acceptance. `git stash pop` restored the patch; all seven tests passed. The deletion tests verified existence before removal and absence afterward, including the request that fills the 8 MiB cap (reviewer-evidence/red.log:266; reviewer-evidence/red.log:310; reviewer-evidence/green.log:8).
- **Regression strength:** the gateway library and existing bulk-delete suite passed independently. The mutation run caught the three boundary-operator changes. Two broader mutants initially failed compilation because deleting code triggers denied unused/dead-code warnings; rerunning only those with `--cap-lints true` compiled them and caught both through tests. Thus all five generated mutants were exercised and caught across the two runs, with no survivor (reviewer-evidence/mutants.log:6; reviewer-evidence/mutants-capped.log:7). This does not claim exhaustive mutation coverage.
- **Quality and declared dependencies:** formatting, Clippy for both affected packages and all their targets, spelling, docs lint, and the 99-page docs render/link audit passed (reviewer-evidence/fmt.log:4; reviewer-evidence/clippy.log:229; reviewer-evidence/typos.log:4; reviewer-evidence/docs-lint.log:3; reviewer-evidence/docs-render.log:4). The first advisory reruns hit a read-only cache lock, an environment limitation (reviewer-evidence/deny-default.log:3). Copying the complete advisory database into this leaf's scratch directory resolved it: default and all-feature advisory scans passed offline, with the parsed policies verified identical except for `db-path`; the all-feature license/bans/sources scan also passed (reviewer-evidence/deny-default-local-cache.log:3; reviewer-evidence/deny-all-advisories-local-cache.log:3; reviewer-evidence/deny-all-other.log:3). The frozen CI independently records non-offline advisory success (gate-logs/C4-ci.log:3610). No declared external dependency remains unexercised.

The frozen evidence was adjudicated within what each log shows:

- **C4-ci: PASS.** Full workspace lint/build/test, dependency walls, conformance, structural guards, and DST completed successfully. The instance-root wrapper was not rerun; the independent checks listed above supplement this frozen result (gate-logs/C4-ci.log:11; gate-logs/C4-ci.log:3618; gate-logs/C4-ci.log:4235).
- **C4-verify: PASS.** Its four failing pre-fix tests and seven passing post-fix tests agree with the independent rerun (gate-logs/C4-verify.log:14; gate-logs/C4-verify.log:61).
- **C4-diff-cov: PASS from frozen evidence.** The measured result is 294/308 instrumentable changed lines, 95.5%, above the 80% floor. The 179 unscored lines and generous treatment of co-located tests limit the metric; it is not proof that every changed line executed (gate-logs/C4-diff-cov.log:10; gate-logs/C4-diff-cov.log:139; gate-logs/C4-diff-cov.log:166).
- **C5-mutants: PASS.** The frozen run reports five caught, corroborated by the independent runs described above (gate-logs/C5-mutants.log:13).
- **T4-batch-review: PASS as recorded.** Its log reports zero blocking findings; it does not contain the individual review passes and was not treated as a substitute for this review (gate-logs/T4-batch-review.log:10).
- **T4-contribution: N/A.** Contribution drafts are intentionally absent at Check; the substantive audit is owed at publish. This deferred row contributes no finding and is separate from the prior-art decision in T4 above (gate-logs/T4-contribution.log:10).
- **host-tikv: PASS from frozen evidence.** The recorded command checked both the TiKV crate and server selection with `tikv,etcd`; both compilation stages completed. This is compilation evidence, not a live TiKV integration claim (gate-logs/host-tikv.log:2).

The remaining history decision is concrete: confirm the merged and closed/rejected-work search for `crates/gateway-s3/src/lib.rs`, `crates/server/tests/s3_key_length_limit.rs`, and `docs/design/architecture/05-building-block-view.md`. The supplied brief names merged #612 and the rejected bulk-refusal designs, which this patch avoids; the local affected-path history investigation returned only the synthetic base (brief.md:155; reviewer-evidence/target-state.log:5). No `INTEGRATION.md` was present in the supplied target to enumerate additional human-only checks (reviewer-evidence/target-state.log:19). The standing rubric was applied without reopening tracked deferrals or inspecting commit sign-offs.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #620 (s3-object-key-length-limit)

**Result: could not refute.** I re-ran the red→green proof, wrote six mutants a weak test would
miss, and probed request-path spellings over the wire. Every attack was caught or behaved as the
brief requires. No NEEDS-HUMAN findings.

- **Red→green re-run (reproduced).** On a scratch copy of `$PDCA_TARGET`: with the fix, 7/7 pass.
  With only `crates/gateway-s3/src/lib.rs` reverted (test kept), 4 tests fail at
  `crates/server/tests/s3_key_length_limit.rs:446` (`assert_refused`): legs 1, 2, 5, 6
  (`:466`, `:498`, `:687`, `:708`), each with `left: 200, right: 400`. The 3 delete legs
  (`:535`, `:611`, `:662`) pass both ways, as the brief's Falsifiability says they should. The
  test drives the production path: the real `S3Gateway::serve` on a loopback socket, the real
  redb + fs backend, and raw signed HTTP. It is not a copy of production logic.
- **Gate claim that overstates (harness wording, not this patch).** `check-gates.json` C4-verify
  says "7 test(s) ran red". Its own log, `gate-logs/C4-verify.log:61`, says
  `3 passed; 4 failed`. Only 4 legs go red, and the other 3 are meant to stay green. The PASS
  verdict still holds (one red leg is enough), but the count is wrong. Nothing for the builder to
  do.
- **Hand-written mutants, each run against the wire test. All caught:**
  - measure the raw wire `key.len()` instead of the decoded key (`crates/gateway-s3/src/lib.rs:1864`)
    → red in `limit_counts_utf8_bytes_of_the_decoded_key_alone` (`s3_key_length_limit.rs:498`);
  - measure `key_name.chars().count()` → red in the same test (the `€` leg);
  - check the raw `bucket.contains('/')` instead of the decoded one (`lib.rs:1853`) → red in legs 5 and 6;
  - measure the flat `object_key.len()` → red in legs 2 and 6;
  - add `if key.len() > 8_000_000 { return Err(Malformed) }` after the empty-key check in
    `parse_delete_request` (`lib.rs:2333`) → red in
    `bulk_delete_removes_the_longest_key_the_body_cap_can_carry` (`s3_key_length_limit.rs:611`).
    This closes the iteration-5 finding: a bound anywhere below the cap now goes red;
  - run the bucket check only for `PayloadHash::Signed` → red in leg 6 (`s3_key_length_limit.rs:708`).
    This closes the iteration-6 finding.
- **Path spellings probed over the wire against the fixed gateway, all correct.**
  `/b%2fa/<1023>` (lowercase hex) → `400 InvalidBucketName`; `/%2Fb/k` → `400 InvalidBucketName`;
  `//b/<1025>` → `400 KeyTooLongError` (`split_bucket_key` trims every leading `/`,
  `lib.rs:591`); `/b/a%2F<1023>` → `400 KeyTooLongError`; `/b/<1025>?x-id=PutObject` →
  `400 KeyTooLongError`; `/b%252Fa/<1023>` → `200`, but it is stored as the literal `b%2Fa/…`,
  not as an alias of `b/a/…`. Routing reads only `parts.uri.path()` (`lib.rs:1675`), and there is
  no virtual-host routing, so the Host header cannot be used to get around the check. No other
  path creates objects: CopyObject answers `501` (`lib.rs:1831`), multipart query forms answer
  `501` through the denylist (`lib.rs:1808`), POST on an object path answers `405`, and
  `crates/proto` has no RPC that writes objects.
- **The 414 claim checked.** The new docs (`lib.rs:498`, and the paragraph in
  `docs/design/architecture/05-building-block-view.md`) say a request target over 65,534 bytes
  gets `414` before the gateway runs. Probed: HEAD with a 65,534-byte path → `404` (it reached the
  gateway); with 65,535 bytes → `414`; HEAD and DELETE of the 70,000-byte key → `414`. The claim is
  exact, so bulk delete really is the only S3 way to remove such a key.
- **Lossy decode (observation; existed before this patch, out of scope, not raised).**
  `percent_decode_utf8` (`lib.rs:3367-3385`) replaces invalid UTF-8 with U+FFFD, which is 3 bytes.
  So `PUT /b/<'%FF' × 342>` (342 raw bytes) is refused `KeyTooLongError`, because the key it
  would store is 1,026 bytes. That matches the Invariant, which limits what gets stored. The lossy
  decode predates #620.
- **Native backend key limits (observation; existed before this patch, not raised).** The new
  docs say a bulk-delete entry is "bounded only by" the 8 MiB body cap (`lib.rs:518-520`). That is
  true of the S3 layer, and the tests run only on redb. FDB and TiKV have their own key-size
  limits, but a key too long for them cannot be stored there at all, so "every stored key stays
  deletable" still holds. #893 tracks native key limits.
- **Test comment claims a little too much (no action needed).** `lib.rs:4903` says the in-crate
  at-limit PUT shows "the form itself is accepted … not a framing or signature failure". But
  `RecordingGateway::put_object_streaming` never reads its `_source` (`lib.rs:4633`), so the
  in-crate test never checks chunk signatures. It only checks the header-level seed signature. The
  wire test's leg 6 (`s3_key_length_limit.rs:708`) does prove the framing works on the real
  backend (HEAD `200` afterwards), so the property is covered.

Attempted to refute: the red→green evidence (re-run; checked that it uses the production path),
the PUT refusal (decoded vs. wire vs. character length, flat-name length, `%2F` and its spelling
variants, leading `//`, query params, all three payload forms, other create paths, Host routing),
and the promise that no delete path gains a length bound (an 8 MB bulk-delete bound mutant; the
414 ceiling). Could not.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No correctness findings introduced by this diff. The PUT-only guards validate the decoded bucket and UTF-8 key length before payload dispatch (`crates/gateway-s3/src/lib.rs:1853`, `crates/gateway-s3/src/lib.rs:1864`). The new tests check backend calls, stored state, alternate payload forms, and deletion through the existing body-cap boundary (`crates/gateway-s3/src/lib.rs:4890`, `crates/server/tests/s3_key_length_limit.rs:611`, `crates/server/tests/s3_key_length_limit.rs:708`).
- No actionable reuse, simplification, or efficiency findings. The implementation reuses decoded components, and the wire tests use existing SigV4 signing and URI-encoding helpers (`crates/gateway-s3/src/lib.rs:1798`, `crates/server/tests/s3_key_length_limit.rs:341`, `crates/server/tests/s3_key_length_limit.rs:408`).

Validation evidence reviewed: frozen CI passed; all seven wire tests passed with the fix and four failed when production changes were reverted; all five tested mutants were caught. No checks were rerun or target files modified.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Confirm merged and closed/rejected prior art by all three affected file paths — the brief records a source-path search, but the supplied target has one synthetic base commit and no remote, so that history cannot be independently substantiated here (brief.md:155; reviewer-evidence/target-state.log:2; reviewer-evidence/target-state.log:5).
- [x] Validation — fitness-to-purpose — Accept the creation-only S3 boundary for intended clients and legacy cleanup — the wire behavior is demonstrated, but native-API keys exceeding the existing bulk-body cap still have no S3 recovery route, as explicitly excluded from this fix (brief.md:81; docs/design/architecture/05-building-block-view.md:139).
- [x] **The deletion guarantee exceeds the deliberately retained limits.** `brief.md:33` promises “Bulk delete reaches every stored key, whatever its length,” and `brief.md:67-70` makes universal deletability an invariant, while `brief.md:112-113` excludes changing the 8 MiB body cap and request-target ceiling. On `$PDCA_TARGET`, `crates/gateway-s3/src/lib.rs:507` sets that cap; `:1920-1921` rejects an overflowing body; and `:2007-2009` returns `400 MalformedXML` before deletion. The native fixture API adds no corresponding key-length check (`crates/server/src/lib.rs:235-256`, `:269-284`; `crates/core/src/metadata.rs:39-40`). A native-seeded `b/<a × 8,388,609>` needs at least 8,388,654 bytes in a one-key DeleteObjects body, so the retained handler refuses it; single-object DELETE also exceeds the brief's stated URI ceiling. The 70,000-byte fixture proves only a case below the body cap. This boundary is also explicit in the maintainer comment (`notes.json:1`): a single key is “bounded only by `MAX_DELETE_BODY_BYTES` (8 MiB).” Revise criterion (3), the invariant, and the planned documentation to promise preservation of existing deletion reachability, with no new 1,024-byte refusal, subject to existing request-format and size limits. Acknowledge the remaining recovery limitation alongside the native-limit deferral; retaining the universal promise would require additional scope beyond this PUT fix.
- [x] **The patch strands objects whose keys are too long for a request line.** `crates/gateway-s3/src/lib.rs:2266` now refuses any bulk-delete `<Key>` over 1,024 bytes. The patch relies on single-object DELETE (`crates/gateway-s3/src/lib.rs:1917`) as the way out for *every* over-limit object (the claims at `:493`, `:1818`, `:1978` and `crates/server/tests/s3_key_length_limit.rs:19`). That way out has a ceiling. The gateway runs on `axum::serve` with hyper defaults (`crates/gateway-s3/src/lib.rs:279`). Hyper parses the request target with the `http` crate (`Cargo.lock:1608-1609`, 1.4.2), whose `Uri` rejects anything over 65,534 bytes, and hyper answers `414` before the gateway runs. **Concrete failing case:** seed `b/<'a' × 70,000>` through `Gateway::put_object`, as leg 4 does. On the patched code, HEAD is `414`, DELETE is `414`, bulk delete is `400 KeyTooLongError`, and the object is still present. On the pre-fix base, HEAD and DELETE are the same `414`s, but bulk delete answers `200` and the object is gone. Such keys can exist. `wyrd put --key` (`crates/server/src/cli.rs:562-568`) takes the key from argv (up to 128 KiB per argument on Linux) into the same metadata store. The CLI has no `delete` subcommand (`crates/server/src/cli.rs:415-416`), so S3 bulk delete was the only exit. Keys that need percent-encoding hit the ceiling sooner, at about 21,800 key bytes, because each byte becomes 3 wire bytes. Leg 4 seeds only a 1,025-byte key (`s3_key_length_limit.rs:319`), so it can't see this. Tagged `[human]` because the builder did what the brief said: criterion (3) demands the bulk refusal, while the invariant and C-1 demand that nothing becomes undeletable, and the two conflict above the URI ceiling. Options: drop the per-entry bound from bulk delete and keep the body cap; raise the bound above what a single DELETE can address; or accept the gap and record it (and/or apply the 1,024-byte limit to `wyrd put --key`, which the brief left out of scope).
- [x] Refusing the whole bulk-delete request (`crates/gateway-s3/src/lib.rs:2270`, `:2114`) makes bucket-emptying tools fail on any batch that contains a legacy over-limit key. Concrete case, run as a probe against the patched tree: I seeded `b/<'a' × 1025>` through `Gateway::put_object` and PUT `k1`, `k2`, `k3`. ListObjectsV2 returns all 4 keys, including the long one. A `DeleteObjects` built from that listing answers `400 KeyTooLongError`, and `k1`, `k2` and `k3` all survive. Any client that lists keys and then deletes them in batches (`aws s3 rm --recursive`, `mc rm -r`, `rclone purge`) fails for every 1,000-key batch holding such a key, not just for that one key. The object itself is not stranded, because single-object `DELETE` still removes it (leg 4), so the brief's invariant holds as written. The patch does what criterion (3) asks. Whether whole-request refusal is the right design, or whether a per-key `<Error><Code>KeyTooLongError</Code>` row (or no bound at all) would be better, is a brief-level call. This probably overlaps the round-1 finding already held for the human ("moot if the human drops the bulk bound").
- [x] **One stored over-limit key blocks bulk cleanup of up to 999 other keys.** `crates/gateway-s3/src/lib.rs:2270` (with `:2114`) refuses a whole DeleteObjects (S3 bulk delete) request if any entry is over 1,024 bytes. Over-limit objects can still exist: those stored before this fix, or written with `wyrd put --key`. `list_container` lists them without filtering (`crates/server/src/lib.rs:714-741`). Concrete case:

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
- By / date: Eduard Ralph / 2026-10-18

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
