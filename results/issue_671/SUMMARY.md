# Result — issue 671 / put-object-flat-ceiling

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: A single PUT that cannot fit one flat chunk map at the gateway's configured chunk
  size is refused with `400 EntityTooLarge`, from the headers, before any of its bytes are
  written. Today nothing enforces the cap. A single PUT is one flat map, and a flat map is sized
  to hold 158 chunk refs (`MAX_MAP_CHUNKS`, "never an over-budget value",
  `crates/core/src/multipart.rs:4755-4759`). That sets the ceiling at 158 × chunk size: 158 MiB at
  the default 1 MiB, about 2.47 GiB at the 16 MiB maximum. Yet no code path compares a PUT
  against it. The constant is referenced only by the server's chunk-size floor
  (`crates/server/src/lib.rs:34`, `:87`, `:100`), and the commit path never checks it. What
  happens to a larger PUT depends on the backend:
  - on FoundationDB, every byte is written, and then the commit fails once the record passes the
    100,000-byte value limit (`crates/server/src/lib.rs:88-96` states this);
  - on redb, which has no value limit, the PUT **publishes** a flat map past the 158-entry budget
    every cap in the code is sized for (`crates/core/src/metadata.rs:546-575`).

  Two gaps keep the cap from being checked up front:
  - **A PUT may omit its length.** A streamed `aws-chunked` upload's object size is
    `x-amz-decoded-content-length`, not its framed `Content-Length`. The gateway reads that header
    only for the `-TRAILER` variants (`crates/gateway-s3/src/sigv4.rs:509`, `:564-590`), and treats
    it as optional there. The plain `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` form (`:549`) never
    reads it. S3 requires it, and AWS's SDKs and CLI always send it.
  - **A declared length is not held to.** Where it is read, it is compared with the streamed bytes
    only after the whole body has arrived (`crates/gateway-s3/src/streaming.rs:532-538`).
- Success criterion: A NEW wire-level test over the in-process gateway, built with
  `.with_chunk_size(8)` so the ceiling is 158 × 8 = **1,264 bytes** (the peer suites use a small
  chunk size the same way):
  (1) a PUT declaring exactly 1,264 bytes (`Content-Length`) succeeds and round-trips byte for
  byte;
  (2) a PUT declaring 1,265 bytes answers `400` with S3 error code `EntityTooLarge`, and nothing is
  stored: HEAD is `404`, and the chunk store directory holds no chunk for it;
  (3) a signed PUT that **declares** `Content-Length: 2147483648` and never sends a body answers
  `400 EntityTooLarge` within a few seconds. That proves the refusal comes from the headers, not
  from reading the body;
  (4) a streamed `aws-chunked` PUT whose `x-amz-decoded-content-length` is 1,265 answers
  `400 EntityTooLarge` the same way, at the header;
  (5) a PUT that declares **no** object length is refused from the headers with
  `411 MissingContentLength` (S3's error for a PUT without a declared length), and nothing is
  stored. That covers a streamed `aws-chunked` upload of **any** variant without
  `x-amz-decoded-content-length` (a small 100-byte body is enough), and a body sent with
  `Transfer-Encoding: chunked` and no `Content-Length`;
  (6) a streamed `aws-chunked` upload that declares `x-amz-decoded-content-length: 1000` (under
  the ceiling) and then streams 1,265 bytes is refused, and nothing is published (HEAD `404`). The
  gateway stops once the declared length is passed, so at most the declared length's worth of
  chunks is written, not the whole stream. One that streams **fewer** bytes than declared is
  refused too;
  (7) **every existing test still tests what it tested.** Two kinds of existing test break under
  this change, and both are migrated so that each keeps its own purpose:
  - **Streamed PUTs with no decoded length** are updated to send it, the way the AWS SDKs do:
    the interop harness in `crates/server/tests/s3_http_wire.rs` (`streaming_seed`, `:1001`) and
    the trailer harness in `crates/server/tests/s3_streaming_trailer.rs`, whose `streaming_seed`
    sends it only optionally (`:356-399`).
  - **Tests whose object is larger than 158 × their own chunk size** would now be refused
    `EntityTooLarge` before reaching the check they exist for. Every peer suite uses an 8-byte
    chunk (ceiling 1,264 bytes) or a 4-byte one (632 bytes). Known cases on `7bf7eb9`:
    `s3_http_wire.rs` (chunk 8, `:65`) streams 4,096 bytes and expects success (`:1070`),
    streams 2,000 tampered bytes and expects `403` (`:1101`, `:1116`), and has the real SDK
    round-trip 9,000 bytes (`:1190-1200`); `s3_streaming_trailer.rs` (chunk 8, `:101`) expects
    4,096- and 3,000-byte uploads to succeed (`:452`, `:504`); and
    `crates/validate/tests/s3_client_roundtrip.rs` (chunk 256 KiB, `:109`, ceiling 39.5 MiB)
    sizes its streaming payload from the host's `tcp_wmem[2]` (`:139-159`): about 37 MiB at the
    4 MiB default, so a host with a larger send buffer would cross the ceiling. Do runs the full
    gate to find any others.
  - **How to migrate:** raise the fixture's chunk size, or shrink its payload, so the object is
    under its ceiling, while keeping what the test needs: an object that spans **several storage
    chunks**, a body that spans **several wire frames**, and each refusal test refused for its
    **own** reason (forged signature, tampered chunk, tampered checksum, unknown algorithm …),
    not for the length or the ceiling. The validate test's payload must stay under its gateway's
    ceiling for any `tcp_wmem[2]` it reads, and should fail with a clear message if it cannot.
    The new boundary tests (1)–(6) keep their own dedicated 8-byte fixture.
  - The real-SDK round trips stay green.

  (8) the `--chunk-size` help text and the M4 blueprint's `--chunk-size` comment state the size
  limits the configured value `c` implies, as numbers for the default and the maximum, **without
  presenting multipart as available**. On `main` and after this slice, every multipart form still
  answers `501 NotImplemented` (`crates/gateway-s3/src/lib.rs:403-413`, `:1777-1783`); the verbs
  land with #668–#670. So:
  - the largest single PUT, 158 × `c`, is stated as **the** limit an object has today;
  - the largest multipart part, `max_part_bytes(c)` = 158 × `c`, and the largest multipart
    object, `MAX_STAGED_CHUNKS` × `c` = 80,896 × `c` in at most 10,000 parts, are stated as the
    limits multipart **will** have once it is supported, not as a remedy available now.

  That is 158 MiB / 158 MiB / about 79 GiB at 1 MiB, and about 2.47 GiB / 2.47 GiB / 1.23 TiB at
  16 MiB. Until multipart ships, the operator's only way to accept a larger object is a larger
  `--chunk-size`, and the help text says so. The `EntityTooLarge` answer to a single PUT names its
  limit in bytes. It does not tell the client to use multipart while this gateway answers
  multipart with `501`;
  (8a) **the buffered path is bounded too.** A direct call to `Gateway::put_object`
  (`crates/server/src/lib.rs:235-256`, the buffered path that does not go through the S3 arm) on
  the same 8-byte-chunk gateway: 1,264 bytes succeeds and `get_object` returns it; 1,265 bytes
  returns `Err`, the chunk count under the fs store's `chunks/` is unchanged, and `get_object`
  returns `None`. The test asserts only `Err` here, not the error's type, so it still compiles on
  the reverted base (where `put_object` exists and stores the 1,265 bytes, which is the red). The
  error's type is pinned by the wire legs, which map it to `EntityTooLarge`;
  (9) the single-PUT ceiling and the multipart part ceiling come from **one** derivation in
  `wyrd-core`, so PutObject and UploadPart can never disagree about what one value holds.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: refusing a single PUT past the flat-map ceiling implied by the configured chunk
  size, from its headers; requiring every PUT to declare its object length, as S3 does; holding
  every streamed upload to its declared length while it streams; the same bound on the buffered
  `Gateway::put_object`; migrating the in-tree tests that send streamed PUTs without the decoded
  length, and those whose objects exceed 158 × their own chunk size, so each keeps its purpose
  (criterion 7); the single-PUT ceiling derived in core beside the multipart part ceiling. Also the operator- and design-facing text that states the size limits the chunk size implies (single PUT, part, multipart object): the `--chunk-size` help, the M4
  blueprint comment, and proposal 0016's PutObject carve-out / out of scope: choosing a chunk size
  per PUT (rejected above); raising `MAX_CHUNK_SIZE` or the gRPC message limit; S3's 5 GiB
  single-PUT limit as a separate runtime check (unreachable while the ceiling is at most about
  2.47 GiB; see Design); segmented maps for single PUTs (multipart-only, 0016 decision 7);
  every multipart verb (#666–#670); the D servers.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 61 mutants tested in 8m: 9 missed, 38 caught, 13 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_671/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.37s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #671’s declared-length enforcement and 158 × configured-chunk-size single-PUT ceiling pass independent red→green review; full CI completion and release fitness remain open.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The approved scope gives falsifiable boundaries for oversized, lengthless and misdeclared uploads, including the buffered entry point; no unresolved implementation scope was found (`brief.md:23`, `target/crates/server/tests/s3_put_object_ceiling.rs:357`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes while retaining the regression test produced nine assertion/timeout failures and two passes, including publication of oversized and lengthless objects (`reviewer-red.log:266`, `reviewer-red.log:351`). |
| C3 Change | PASS | Both public PUT paths reject unpublishable sizes before storage work, and under-declaration cannot bypass admission; the persisted representation is unchanged (`target/crates/server/src/lib.rs:258`, `target/crates/server/src/lib.rs:417`, `target/crates/server/src/lib.rs:960`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the existing optional-TiKV advisory failure and obtain complete CI evidence — independent regression tests pass 11/11, but frozen CI stops before conformance, statics, orchestrator guard and DST, and diff coverage was not measured (`reviewer-green.log:130`, `gate-logs/C4-ci.log:6567`, `target/xtask/src/main.rs:1683`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | PASS | Admission plus incremental length enforcement addresses the oversized-map cause on both entry points; lazy decoder startup removes eager body consumption, so the capability-probe smell test does not fire (`target/crates/server/src/lib.rs:291`, `target/crates/gateway-s3/src/streaming.rs:200`, `target/crates/gateway-s3/src/streaming.rs:369`). |
| T1 Structure | PASS | The protocol layer keeps the backend-neutral object seam, while the two one-value ceilings share the core derivation; the living architecture documents the changed contract (`target/crates/gateway-core/src/lib.rs:324`, `target/crates/core/src/multipart.rs:4784`, `target/docs/design/architecture/05-building-block-view.md:137`). |
| T2 Shape | PASS | The change stays within admission, stream accounting, fixture migration and the authorized documentation; the added internal dependency is test-only, preserving the validator’s production dependency boundary (`target/crates/validate/Cargo.toml:42`, `target/crates/validate/tests/s3_client_roundtrip.rs:171`). |
| T3 Runtime | PASS | Independent wire tests confirm timely header refusals, no chunks for rejected oversized requests, bounded writes for overruns and no publication on length mismatch; migrated SDK/trailer and client streaming tests also pass (`target/crates/server/tests/s3_put_object_ceiling.rs:418`, `target/crates/server/tests/s3_put_object_ceiling.rs:539`, `reviewer-related.log:153`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit is mandatory at publish, so this deferred gate is neither a pass nor a finding (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The patch respects the approved deployment-size decision and preserves existing tests’ refusal reasons; affected-path history and closed/unmerged PR files were independently checked, including merged #848 and closed #647 (`reviewer-prior-art-paths.log:171`, `reviewer-prior-art-paths.log:385`, `target/crates/server/tests/s3_streaming_trailer.rs:843`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm release suitability before multipart exists — objects above 158 MiB by default, or about 2.47 GiB at the maximum setting, have no supported upload route, and lengthless PUTs now receive 411 (`brief.md:326`, `target/crates/server/src/cli.rs:529`, `target/docs/design/architecture/m4-first-deployment-blueprint.md:1151`). |

No patch-specific implementation defect was established. The outstanding verification issue belongs to the repository’s existing dependency gate; this advisory report does not override its failure.

- **Independent execution supports the intended behavior.** In the disposable `$PDCA_TARGET`, `git stash push` retained the new untracked test and exposed the pre-fix behavior; `git stash pop` restored the patch. `cargo test --offline --locked -p wyrd-server --test s3_put_object_ceiling` failed 9/11 before the fix and passed 11/11 afterward. The additional server unit, HTTP wire, trailer, gateway-S3 unit and validator client-round-trip suites passed: 212 tests in total, including the 11 boundary tests (`reviewer-green.log:88`, `reviewer-green.log:113`, `reviewer-green.log:130`, `reviewer-green.log:147`, `reviewer-related.log:126`, `reviewer-related.log:153`). This exercised real Rust builds and loopback HTTP with redb and filesystem storage; the fixture reproduces the forbidden pre-fix publication rather than excluding it by construction. All 18 target postimages match `patch.diff` after restoration.
- **Full verification remains incomplete for identified reasons.** The frozen CI log records successful formatting, clippy, build, workspace tests, dependency hygiene and default-feature dependency checks, then fails on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 through optional TiKV’s existing rustls 0.21.12 dependency (`gate-logs/C4-ci.log:3623`, `gate-logs/C4-ci.log:6559`, `gate-logs/C4-ci.log:6574`, `gate-logs/C4-ci.log:6604`; actual lockfile location: `target/Cargo.lock:3294`). The patch changes only a local dev-dependency edge in that lockfile (`patch.diff:5`). This is not a PUT implementation defect or a missing-tool explanation for the frozen red. The reviewer’s advisory scan could not acquire its read-only database lock (`reviewer-advisories.log:1`), and local rustfmt is absent (`reviewer-fmt.log:1`); those checks are adjudicated from the frozen log, not claimed as independently repeated. Independent `typos`, docs lint, docs render/link audit and `git diff --check` passed; both named external dependencies were exercised (`reviewer-doc-lint.log:1`, `reviewer-doc-render.log:2`). The separate TiKV/etcd clippy log shows actual compilation completing, not a service-level test (`gate-logs/host-tikv.log:211`).
- **Coverage, mutation and prior-art evidence have bounded conclusions.** The coverage wrapper reports that the patch does not apply to its `origin/main`; coverage is therefore unmeasured, not zero and not evidence that the supplied target fails to compile (`gate-logs/C4-diff-cov.log:10`). The supplied target itself applies and compiles correctly. The mutation log records 9 missed, 38 caught, 13 unviable and 1 timeout; it is not a clean campaign (`gate-logs/C5-mutants.log:13`, `gate-logs/C5-mutants.log:23`). Several listed survivors change behavior asserted by the server wire suite, such as returning zero from the ceiling or removing the `EntityTooLarge` mapping; the log does not establish that those cross-crate assertions ran against each mutant. Other survivors affect diagnostic text or defensive parsing/error mapping. No runtime defect follows from these survivors alone, and mutation coverage is not claimed complete. The frozen batched-review log reports zero findings but supplies no detailed pass content; this review independently inspected the diff (`gate-logs/T4-batch-review.log:10`). GitHub history was queried for every affected path, and all closed/unmerged PR file lists were intersected with those paths. Relevant prior work includes [merged #848](https://github.com/getwyrd/wyrd/pull/848), the floor change `f8634ded300b`, and [closed #647](https://github.com/getwyrd/wyrd/pull/647), whose closure explicitly requests smaller scope (`reviewer-prior-art-paths.log:177`, `reviewer-prior-art-paths.log:385`, `reviewer-prior-art-647.log:2`). No duplicate accepted solution was established.

The supplied standing rubric was applied, including protocol input, seam direction, documentation currency, test fidelity and the prohibition on independent DCO findings. No `INTEGRATION.md` exists among the supplied inputs or target files, so any additional project-only §4 sign-off items remain for the human’s validation checklist. No visual/manual reproduction is needed for the behavior exercised here.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 671 / put-object-flat-ceiling

Verdict: the S3 fix holds against every wire input I tried. The red→green proof is real: it runs through the actual wire and `Gateway`, the 9 red legs fail for the right reasons, and the chunk counter is not vacuous. I refuted three things: the claim that the guard covers every single-PUT path, the test coverage of the server-side length guard, and the claim that the related docs are current. All probes ran on a scratch copy of `$PDCA_TARGET` (cargo 1.96) and have been removed.

## Findings

- NEEDS-HUMAN [human] — **`wyrd put` still publishes over-budget flat maps; the patch's "every single-PUT path" claim is false.** `crates/server/src/lib.rs:290` says "Every single-PUT path calls this before it writes anything", and the brief's invariant says "on any backend". But `cmd_put` (`crates/server/src/cli.rs:590`, `--chunk-size` with no bounds) writes through `write::write_new_object` (`crates/server/src/cli.rs:655` local path, `:2267` cluster path → `crates/core/src/write.rs:353-366`) and never calls `admit_single_put`. Concrete case on the patched tree: a 1,265-byte file with `wyrd put payload --key over --data-dir D --chunk-size 8` printed `put ok: key=over inode=1 chunks=159 bytes=1265`, and `wyrd get over` returned it byte-identical (redb). At the default 1 MiB chunk, any file over 158 MiB does the same. On FDB it writes every fragment and then fails the commit, which is the exact behaviour #671 set out to remove. The brief's SELF-TEST named only three paths, so this is a scope call. The fix is small either way: put the check in core `write_new_object` / `write_new_object_placed` (beside `max_single_put_bytes`) so no caller can skip it. Or soften the comment at `:290` and file a follow-up.

- NEEDS-HUMAN [impl] — **No test pins the server-side length guard.** In the scratch copy I replaced `HashingSource::new(DeclaredLengthSource::new(source, declared_len))` at `crates/server/src/lib.rs:418` with `HashingSource::new(source)`. All 45 tests in `s3_put_object_ceiling`, `s3_http_wire`, `s3_streaming_trailer` and `e2e` still passed. On the S3 wire, hyper frames single-shot bodies and the aws-chunked decoder (`crates/gateway-s3/src/streaming.rs:370`) catches over-runs first, so the server guard is never reached. The seam contract at `crates/gateway-core/src/lib.rs:324-329` ("holds `source` to `declared_len`") is the only thing that protects any other caller. Concrete failing case with the guard unwired: on the 8-byte gateway, `gateway.put_object_streaming("wyrd-bucket/seam-overrun", <1,265 bytes in 64-byte frames>, 1_000, ContentHash::Unverified, None)` returned `Ok`, published 1,265 bytes, and left 159 chunks on disk. With the patch as written it returns `Err`, publishes nothing, and leaves 120 chunks, so the code is right; only the proof is missing. Add this as a seam-level integration test. `s3_http_wire.rs`'s `streaming_behaviour` already calls the seam directly, so it fits there.

- NEEDS-HUMAN [impl] — **The new `LengthMismatch → 400 IncompleteBody` mapping is dead on the wire and untested.** It lives at `crates/gateway-s3/src/lib.rs:3593-3597`. C5 shows that deleting the arm survives, and nothing in the tree asserts `IncompleteBody`. The one wire case S3 uses that code for still answers differently. Probe: an `UNSIGNED-PAYLOAD` PUT that declares `Content-Length: 1000`, sends 500 bytes, and half-closes gets `500 InternalError` (HEAD stays `404`, so it is still refused, not accepted). Low severity: either map hyper's incomplete-body error onto `IncompleteBody`, or drop the code and say the case answers 500.

- NEEDS-HUMAN [human] — **Draft proposal 0017 now contradicts the code.** The brief knew about this (prior-art check) but scoped only 0016. These passages in `docs/design/proposals/draft/0017-blackbox-validation-tool.md` are now false:
  - `:291-292`: "5 GiB … **must succeed**" and `EntityTooLarge` "**unimplemented today**";
  - `:313`: per-PUT `chunk_size_effective = max(...)`;
  - `:323`: "the gateway overrides it above the flat-map ceiling";
  - `:326`: the guard is "not a routine path".

  After this patch, every PUT over 158 × c is refused, including 5 GiB at every legal chunk size. Decide whether to correct 0017 in this PR (it is a draft) or file a follow-up. Otherwise the validate tool's size matrix gets built on a premise this patch removed.

- NEEDS-HUMAN [impl] — **The evidence was taken on a stale base.** `gate-logs/C4-diff-cov.log`: "patch.diff does not apply on origin/main — the bundle is stale". C4-verify, C4-ci and C5 all measured base `596249a`. The brief lists conflicts with #620, #666 and #768 on this exact PUT arm (`crates/gateway-s3/src/lib.rs:1827-1864`) and seam (`crates/gateway-core/src/lib.rs:330`). Rebase, then re-run C4-verify. In particular, re-check the order of the new 411/EntityTooLarge refusals against #620's header-time refusal once both are on the same arm.

- NEEDS-HUMAN [human] — **The C4-ci gating red is not caused by this diff.** The only failing step is `cargo deny … check advisories`: RUSTSEC-2026-0349 plus the rustls `Debug`-secrets advisory, both in `rustls 0.21.12` via `tikv-client 0.4.0` (`gate-logs/C4-ci.log:3520-3597`). This patch's `Cargo.lock` change only adds `wyrd-core` to `wyrd-validate`'s dependencies. fmt, clippy, build and every test passed in the same log, including all 11 new ceiling tests. The human should waive it or fix the advisory separately; it is not a refutation of the fix.

- Gate wording (not a defect): C4-verify reports "11 test(s) ran red", but its own log shows 9 failed and 2 passed on the reverted base. The two passes are `a_put_at_the_ceiling_round_trips` and the signer sanity check, which is expected. The 9 reds are the right kind: 200 where 400/411 was expected, legs (3) and (4) hitting the 10 s bound, and the buffered PUT returning `Ok`.

## Attacks I tried and could not land

- **Vacuous chunk counter.** It is not vacuous. A probe saw exactly 158 chunk directories after a 1,264-byte buffered PUT. `count_chunk_dirs` (`crates/server/tests/s3_put_object_ceiling.rs:130`) matches the fs store's `root/<32-hex>/<05>.frag` layout (`crates/chunkstore-fs/src/lib.rs:597-599`).
- **`Expect: 100-continue`** with `Content-Length: 5000`: the first status line is `HTTP/1.1 400`, with `EntityTooLarge`. No `100` is sent and the body is never read.
- **Body forms that could slip past.** `UNSIGNED-PAYLOAD` past the ceiling gets `400 EntityTooLarge` and writes 0 chunks. An empty PUT with `Content-Length: 0` still returns 200. An aws-chunked body inside an outer `Transfer-Encoding: chunked`, with a decoded length, returns 200, so the TE→411 rule does not break that SDK form.
- **Overflow and grammar.** A `u64::MAX` declared length is refused by `admit_single_put`. Values past `u64`, `+5`, and padded values are `Malformed` before the body is read.
- **Other front doors.** The only production `ObjectGateway` implementer is `crates/server/src/lib.rs:384`, and DST does not call the PUT seam.
- **Unsigned decoded length.** An `x-amz-decoded-content-length` sent outside `SignedHeaders` is trusted (200). But every chunk is signed and the body is held to that length, so tampering with it can only cause a refusal.
- **Migrated validate test.** `chunk_for_single_put` (`crates/validate/tests/s3_client_roundtrip.rs:172`) keeps the chunk at about 5% of the window, so the window property still holds. The other legs use fixed objects of 2 MiB or less.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No introduced correctness bugs found in the reviewed diff. One optional reuse cleanup:

- NEEDS-HUMAN [impl] — Low priority: `crates/server/tests/s3_put_object_ceiling.rs:623` adds a padded Base64 encoder already provided by `wyrd_gateway_s3::checksum::base64_encode` at `crates/gateway-s3/src/checksum.rs:150`. Use that public helper at the trailer-construction call instead of maintaining another implementation. The existing helper is unchanged by this diff, so reuse preserves compilation on the reverted base; this test exercises missing-length rejection, not Base64 correctness.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Resolve the existing optional-TiKV advisory failure and obtain complete CI evidence — independent regression tests pass 11/11, but frozen CI stops before conformance, statics, orchestrator guard and DST, and diff coverage was not measured (`reviewer-green.log:130`, `gate-logs/C4-ci.log:6567`, `target/xtask/src/main.rs:1683`, `gate-logs/C4-diff-cov.log:10`).
- [x] Validation — fitness-to-purpose — Confirm release suitability before multipart exists — objects above 158 MiB by default, or about 2.47 GiB at the maximum setting, have no supported upload route, and lengthless PUTs now receive 411 (`brief.md:326`, `target/crates/server/src/cli.rs:529`, `target/docs/design/architecture/m4-first-deployment-blueprint.md:1151`).
- [x] **`wyrd put` still publishes over-budget flat maps; the patch's "every single-PUT path" claim is false.** `crates/server/src/lib.rs:290` says "Every single-PUT path calls this before it writes anything", and the brief's invariant says "on any backend". But `cmd_put` (`crates/server/src/cli.rs:590`, `--chunk-size` with no bounds) writes through `write::write_new_object` (`crates/server/src/cli.rs:655` local path, `:2267` cluster path → `crates/core/src/write.rs:353-366`) and never calls `admit_single_put`. Concrete case on the patched tree: a 1,265-byte file with `wyrd put payload --key over --data-dir D --chunk-size 8` printed `put ok: key=over inode=1 chunks=159 bytes=1265`, and `wyrd get over` returned it byte-identical (redb). At the default 1 MiB chunk, any file over 158 MiB does the same. On FDB it writes every fragment and then fails the commit, which is the exact behaviour #671 set out to remove. The brief's SELF-TEST named only three paths, so this is a scope call. The fix is small either way: put the check in core `write_new_object` / `write_new_object_placed` (beside `max_single_put_bytes`) so no caller can skip it. Or soften the comment at `:290` and file a follow-up.
- [x] **No test pins the server-side length guard.** In the scratch copy I replaced `HashingSource::new(DeclaredLengthSource::new(source, declared_len))` at `crates/server/src/lib.rs:418` with `HashingSource::new(source)`. All 45 tests in `s3_put_object_ceiling`, `s3_http_wire`, `s3_streaming_trailer` and `e2e` still passed. On the S3 wire, hyper frames single-shot bodies and the aws-chunked decoder (`crates/gateway-s3/src/streaming.rs:370`) catches over-runs first, so the server guard is never reached. The seam contract at `crates/gateway-core/src/lib.rs:324-329` ("holds `source` to `declared_len`") is the only thing that protects any other caller. Concrete failing case with the guard unwired: on the 8-byte gateway, `gateway.put_object_streaming("wyrd-bucket/seam-overrun", <1,265 bytes in 64-byte frames>, 1_000, ContentHash::Unverified, None)` returned `Ok`, published 1,265 bytes, and left 159 chunks on disk. With the patch as written it returns `Err`, publishes nothing, and leaves 120 chunks, so the code is right; only the proof is missing. Add this as a seam-level integration test. `s3_http_wire.rs`'s `streaming_behaviour` already calls the seam directly, so it fits there.
- [x] **The new `LengthMismatch → 400 IncompleteBody` mapping is dead on the wire and untested.** It lives at `crates/gateway-s3/src/lib.rs:3593-3597`. C5 shows that deleting the arm survives, and nothing in the tree asserts `IncompleteBody`. The one wire case S3 uses that code for still answers differently. Probe: an `UNSIGNED-PAYLOAD` PUT that declares `Content-Length: 1000`, sends 500 bytes, and half-closes gets `500 InternalError` (HEAD stays `404`, so it is still refused, not accepted). Low severity: either map hyper's incomplete-body error onto `IncompleteBody`, or drop the code and say the case answers 500.
- [x] **Draft proposal 0017 now contradicts the code.** The brief knew about this (prior-art check) but scoped only 0016. These passages in `docs/design/proposals/draft/0017-blackbox-validation-tool.md` are now false:
- [x] **The evidence was taken on a stale base.** `gate-logs/C4-diff-cov.log`: "patch.diff does not apply on origin/main — the bundle is stale". C4-verify, C4-ci and C5 all measured base `596249a`. The brief lists conflicts with #620, #666 and #768 on this exact PUT arm (`crates/gateway-s3/src/lib.rs:1827-1864`) and seam (`crates/gateway-core/src/lib.rs:330`). Rebase, then re-run C4-verify. In particular, re-check the order of the new 411/EntityTooLarge refusals against #620's header-time refusal once both are on the same arm.
- [x] **The C4-ci gating red is not caused by this diff.** The only failing step is `cargo deny … check advisories`: RUSTSEC-2026-0349 plus the rustls `Debug`-secrets advisory, both in `rustls 0.21.12` via `tikv-client 0.4.0` (`gate-logs/C4-ci.log:3520-3597`). This patch's `Cargo.lock` change only adds `wyrd-core` to `wyrd-validate`'s dependencies. fmt, clippy, build and every test passed in the same log, including all 11 new ceiling tests. The human should waive it or fix the advisory separately; it is not a refutation of the fix.
- [x] Low priority: `crates/server/tests/s3_put_object_ceiling.rs:623` adds a padded Base64 encoder already provided by `wyrd_gateway_s3::checksum::base64_encode` at `crates/gateway-s3/src/checksum.rs:150`. Use that public helper at the trailer-construction call instead of maintaining another implementation. The existing helper is unchanged by this diff, so reuse preserves compilation on the reverted base; this test exercises missing-length rejection, not Base64 correctness.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [x] **The existing-test migration misses uploads that exceed the new cap.** `brief.md:75-81` prescribes adding decoded-length headers, preserving each refusal's original reason, and keeping real-SDK round trips green unchanged. On the target, `crates/server/tests/s3_http_wire.rs:65` configures 8-byte chunks, but its successful streaming upload is 4,096 bytes (`:1070`), its tampered-signature upload is 2,000 bytes and expects 403 (`:1101`, `:1116`), and its real-SDK round trip uploads 9,000 bytes (`:1190-1200`). All exceed the proposed 1,264-byte ceiling. The trailer suite similarly uses 8-byte chunks (`crates/server/tests/s3_streaming_trailer.rs:101`) and expects successful 4,096- and 3,000-byte uploads (`:452`, `:504`). Adding truthful length headers makes these fail early with EntityTooLarge, before their intended checks. Revise the migration scope to adjust fixture chunk sizes or payloads while retaining multiple storage chunks, multiple wire frames, and the original refusal reasons; retain the dedicated 8-byte fixture for the new boundary tests.
- [x] **The success criterion cannot detect an unfixed buffered PUT path.** `brief.md:104-113` explicitly binds the invariant to `Gateway::put_object`, and `:232-233` includes that path, but the new acceptance cases (`:53-81`) exercise only HTTP. The target's buffered method independently calls `plan_write`, writes fragments, and commits (`crates/server/src/lib.rs:235-256`); the HTTP handler instead calls `put_object_streaming` (`crates/gateway-s3/src/lib.rs:1829-1864`). Consequently every proposed wire case can pass while buffered PUT still publishes a 159-chunk map. Add an explicit direct buffered acceptance case at the ceiling and one byte above it, asserting refusal before fragment writes and no publication. This uses an existing API and can also serve the brief's baseline-compilation requirement.
- [x] **The advertised multipart escape route is not available on the declared independent base.** `brief.md:82-91` requires operator-facing multipart capacities and an error directing oversized PUTs to multipart; `:117-118` says the multipart verbs are not needed, and `:140` excludes them. On the target, `uploads`, `uploadId`, and `partNumber` remain unsupported (`crates/gateway-s3/src/lib.rs:403-413`), and the object route returns 501 NotImplemented (`:1777-1783`). The core capacity constants therefore establish future limits, not a working alternative for an operator deploying this change alone. Revise the help/error/documentation requirements to identify multipart availability as pending the implementing slices, or explicitly state the release ordering needed before presenting it as an available remedy. The cap itself need not acquire a build dependency.
- [x] size backstop — this slice is behaving oversized: patch is 125 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (#671 item 4): add a seam-level test that over-runs `put_object_streaming` past its declared length, pinning the server-side length guard (`crates/server/src/lib.rs:418`).
- Follow-up (#671 item 5): `LengthMismatch → 400 IncompleteBody` is unreachable on the wire; a short Content-Length body answers `500` — map hyper's incomplete-body error to `IncompleteBody` or drop the arm.
- Follow-up (#671 item 6): correct draft proposal 0017 (`:291-292`, `:313`, `:323`, `:326`) to the per-PUT ceiling of 158 × chunk size.
- Follow-up (#671 item 9): reuse `wyrd_gateway_s3::checksum::base64_encode` in `s3_put_object_ceiling.rs:623` instead of the duplicate encoder.
