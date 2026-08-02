# Result — issue 508 / multipart-upload

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the full S3 multipart-upload verb set — `CreateMultipartUpload`, `UploadPart`,
- Success criterion: five legs, all over the wire against an in-process gateway. Legs A, B
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the six multipart verbs, their records, their publication path and their staged

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo build --workspace --exclude wyrd-dst --all-targets` failed with exit status: 101
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 925 mutants tested in 58m: 312 missed, 213 caught, 395 unviable, 5 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 32 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #508’s implementation of the complete S3 multipart-upload verb set, fenced publication and retirement lifecycle, segmented maps, and ordinary-PUT sizing.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision has a concrete protocol oracle for atomic drain fencing, fence release, and verb-by-state outcomes (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:658`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:969`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:993`). |
| C2 Reproduction (red pre-fix) | PASS | On a pristine target base, both added test binaries compiled and all 22 assertions failed (0/8 lifecycle, 0/14 wire); their discriminating oracles include exact routing (`crates/server/tests/s3_multipart_upload.rs:823`) and positive drain safety (`crates/server/tests/s3_multipart_lifecycle.rs:562`). |
| C3 Change | FAIL | The specified change is incomplete: a drain-fenced UploadPart deliberately refuses fleet-wide instead of re-planning (`crates/server/src/multipart.rs:459`; required at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:658`), and rejected Complete discards the fence-release commit outcome (`crates/server/src/multipart.rs:746`), so a drain can halt multipart writes and a failed release can leave a session stuck in `Completing`. |
| C4 Verification (red→green) | PASS | The same 22 tests went from 0/22 on base to 22/22 with the patch, and an isolated `cargo xtask ci` passed build, tests, deny, conformance, statics, orchestrator checks, and seeded DST; the gate’s reported build failure did not reproduce (`crates/server/tests/s3_multipart_upload.rs:1846`, `crates/server/tests/s3_multipart_lifecycle.rs:829`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether executor ownership can be required or injected so the runtime capability probe is unnecessary — its silent no-runtime path moves inline retirement entirely to #625 (`crates/server/src/multipart.rs:164`); additionally, `scripts/mutants-in-diff` was unavailable, so only the 925-mutant enumeration, not the reported outcomes, was reproduced. |
| T1 Structure | NEEDS-HUMAN | Maintainers must decide whether the planned 44-file, 14,117-line cross-plane slice remains one reviewable unit or re-enters Plan for splitting — persisted formats, gateway behavior, and destructive maintenance now change together, increasing the chance that lifecycle coupling escapes review. |
| T2 Shape | PASS | The structural decision is supported by byte-identity coverage for legacy and owned staging records plus a native cursor test beyond the old scan cap (`crates/core/src/metadata.rs:1060`, `crates/metadata-redb/tests/scan.rs:107`), all of which passed independently. |
| T3 Runtime | NEEDS-HUMAN | A human must validate a stock-client ≥8 GiB round-trip on the deployed topology — the AWS CLI, live endpoint, and required disk topology were absent, so evidence is in-process only; with `S3_ENDPOINT` and `BUCKET` set, run `p="${PDCA_SCRATCH:-${TMPDIR:-/tmp}}/pdca-reviewer-508-8g"; truncate -s 8G "$p.in"; aws --endpoint-url "$S3_ENDPOINT" s3 cp "$p.in" "s3://$BUCKET/pdca-508-8g"; aws --endpoint-url "$S3_ENDPOINT" s3 cp "s3://$BUCKET/pdca-508-8g" "$p.out"; cmp "$p.in" "$p.out"; rm -f "$p.in" "$p.out"` and require `cmp` success. |
| T4 Contribution | NEEDS-HUMAN | A human must reconcile the unavailable `scripts/review-branch` batch before relying on its reported 32 findings — independently, all 44 affected paths had no non-main history and 252 closed PRs yielded no rejected overlap, so prior-art novelty is mechanically clear but the rubric finding set is not. |
| T5 Judgment | NEEDS-HUMAN | The architecture board must reconcile durable group identity and enforce the one-merge release with #625/#633 — the draft requires an independent group nonce (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:508`) but later equates it to the upload id (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2318`), while releasing before the reaper violates its normative order (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:236`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Product sign-off must decide whether the integrated #508/#625/#633 stack satisfies real large-object and operator-lifecycle needs — automated small-topology success cannot establish deployed ≥8 GiB interoperability, recovery, or drain behavior. |

### Advisory — adversary

# Adversarial review — issue #508 (multipart-upload), iteration 7

Advisory only; I never gate. Everything below is grounded on the target source at
`$PDCA_TARGET` (`/home/eddie/development/wyrd/wyrd.pdca-wt`, patch applied on `22d71b4`).

## What I tried to refute and could not

- **The red→green evidence is real, and I reproduced it independently.** I exported the base
  tree (`git archive HEAD`) to scratch, dropped in *only* the two added test files, and ran
  them: both binaries **compile on `origin/main`** and **all 22 tests fail by assertion**
  (`s3_multipart_lifecycle` 0 passed / 8 failed; `s3_multipart_upload` 0 passed / 14 failed) —
  not a build error, which was the brief's named gate hazard. With the patch, in a clean
  `CARGO_TARGET_DIR`, all 22 pass. The tests drive the real gateway over HTTP with a real
  `aws-sdk-s3` client and the real custodian passes; only the metadata *backend* is a double,
  and `crates/metadata-redb/src/lib.rs:144` / `-fdb:1817` / `-tikv:1047` / `dst/tests/support/mod.rs:312`
  do carry native `scan_page` implementations, so the seam is not a shim.
- I attacked the ETag composition (`crates/core/src/multipart.rs:1038`), the
  `?part%4Eumber=1` fence (`crates/gateway-s3/src/lib.rs:529,1955`), the `Complete` XML
  grammar (`:2723-2808`), `drain_records`' 4,001-part convergence
  (`crates/core/src/multipart.rs:2762`, regression test `:3415`), the segmented-map resolver's
  reader-safety (`crates/core/src/multipart.rs:814-837`), and the `strict_positions`/
  `observe_positions` orphan fan-out (`:2504-2546`). **I could not break any of them.**

## Findings

- **NEEDS-HUMAN — the C4 red is an environment fault, not a broken patch (verdict on C4
  provisional).** `check-gates.json` reports `cargo build --workspace --exclude wyrd-dst
  --all-targets` exiting 101 with `no MultipartFault in the root of wyrd_gateway_core` and
  `scan_page is not a member of trait MetadataStore` — although those symbols are plainly
  present at `crates/gateway-core/src/lib.rs:420` and `crates/traits/src/lib.rs:813`, ungated
  by any `cfg`. I reproduced the failure twice **in the repo's pre-existing 90 GB `target/`
  cache**, and it disappears completely in a clean scratch `CARGO_TARGET_DIR`:
  `cargo check --workspace --exclude wyrd-dst --all-targets`, `cargo clippy … -D warnings` and
  `cargo fmt --all --check` are all **green** there. The cache holds rlibs whose mtimes
  (16:25) post-date the sources they claim to be built from (`crates/traits/src/lib.rs`,
  15:40:30), i.e. a mtime-preserving patch restore left cargo's fingerprints stale. A human
  should re-run `xtask ci` after `cargo clean` (or with a fresh target dir) before treating
  C4 as a genuine red. This is issue-#236 territory, not a refutation.
- **NEEDS-HUMAN [impl] — `CreateMultipartUpload` answers `503 SlowDown` on a completely empty
  store at ordinary client concurrency.** `crates/server/src/multipart.rs:46` sets
  `MINT_ATTEMPTS = 4`, documented as "how many upload ids to try before giving up on a
  **collision**" (a 2^-128 event) — but the same 4-attempt budget is the *only* retry for the
  globally serialized admission CAS at `crates/core/src/multipart.rs:1436-1443`
  (`require(mpuctl == prior)`, one key for the whole store). Every concurrent Create but one
  loses each round, so the k-th concurrent creator needs O(k) attempts; at 4 it falls through
  to `Err(SlowDown)` (`crates/server/src/multipart.rs:297,320-322`). Measured against the real
  `create_session` over `RedbMetadataStore::in_memory`, replicating that loop verbatim:
  concurrency 2 → 0 refusals, 4 → 0, **8 → 2 refused, 10 → 3 refused, 16 → 3 refused**, with
  `mpuctl` reading `{"count":13,"staged":13,"max_sessions":74,"max_records":65536}` — i.e. no
  bound anywhere near reached. aws-cli's default `max_concurrent_requests` is 10, so
  `aws s3 sync`/`cp --recursive` over several large files hits this on an idle cluster. The
  brief's Impact section justifies `503 SlowDown` as "too many in-flight parts or open
  sessions"; this one fires with the store empty. No test covers concurrent *Create*
  (`concurrent_part_uploads_do_not_conflict_with_each_other`,
  `crates/server/tests/s3_multipart_upload.rs:1986`, covers concurrent UploadPart only).
  Separate the CAS-contention retry from the id-collision retry and give it a real bound (plus
  jittered backoff).
- **NEEDS-HUMAN [impl] — the "operator-visible startup signal" is neither at startup nor
  silenceable.** `Gateway::with_reaper` (`crates/server/src/lib.rs:201`) has **zero callers in
  the entire tree** — the `wyrd s3` role builds its gateway at `crates/server/src/cli.rs:1588`
  and `:2066-2099` with a bare `Gateway::new(...)` — so `reaper_expected` is permanently
  `false` in every real deployment. Consequences, both concrete: (i) once #625 lands and the
  operator *is* running `wyrd custodian`, the gateway still emits
  `warn!(s3_multipart_reaper_absent)` (`crates/server/src/lib.rs:233-243`) on **every**
  `CreateMultipartUpload` (`crates/server/src/multipart.rs:291,308`) — `aws s3 cp --recursive`
  of 1,000 files logs 1,000 warnings, the alert-fatigue shape the signal exists to avoid; and
  (ii) it is not a *startup* signal at all — a misconfigured `wyrd s3` that has not yet served
  a Create emits nothing, so the brief's Scope item ("an operator-visible startup signal on
  the `wyrd s3` role") is not met. Wire a CLI/config knob through to `with_reaper`, and emit
  the signal once at role startup.
- **NEEDS-HUMAN — GC's orphan-ledger walk loses the `SCAN_CAP` heap bound exactly where this
  patch makes the population explode.** `crates/custodian/src/gc.rs:720-738` replaces the
  base's single `scan(ORPHAN_PREFIX)` (SCAN_CAP-bounded, fails loud past 2^20 —
  `crates/traits/src/lib.rs:286`, whose doc says the cap exists to "bound a gateway's heap
  against a pathological prefix") with an **unbounded** `loop { scan_page(...) }` that
  materialises every entry into one `HashMap`. This patch's own arithmetic makes that
  population large: a maximal segmented generation is `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS`
  = 48,672 chunks (`crates/core/src/multipart.rs:124`), ~438 K fragment positions at
  RS(6,3), and `crates/core/src/multipart.rs:2485` itself cites ~1.78 M. Retiring a handful of
  large objects therefore pushes `orphan_leases` into a multi-hundred-MB heap allocation in the
  custodian, where the base would have failed loud. Bounding it needs a design call (a page
  budget changes what one GC pass may conclude), which is why this is not tagged `[impl]`.
- **NEEDS-HUMAN [impl] — `GET`/`HEAD /b/k?partNumber=N` regresses from `501 NotImplemented` to
  `400 InvalidArgument`.** `crates/gateway-s3/src/lib.rs:629` returns
  `Invalid("`partNumber` requires `uploadId`")` for *any* method carrying `partNumber` without
  `uploadId`, and `:1955` intercepts before the denylist. But `GET ?partNumber=N` is a **real,
  well-formed S3 operation** (part-ranged GetObject, used by parallel downloaders); on the base
  it was cleanly `501` because `partNumber` was denylisted
  (`git show HEAD:crates/gateway-s3/src/lib.rs`, `UNSUPPORTED_SUBRESOURCES`). Telling a client
  its valid request is malformed is the wrong refusal, and the brief only demanded the `400`
  for `PUT /b/k?partNumber=1`. Route non-PUT `?partNumber` without `uploadId` to `501`.
- **NEEDS-HUMAN — a `<ChecksumCRC32>` (or any `Checksum*`) child in the Complete body refuses
  the whole assembly `400 MalformedXML`.** `crates/gateway-s3/src/lib.rs:2754`. The refusal is
  deliberate and documented (an unverifiable integrity condition must not be silently
  dropped), and per-part checksums are out of scope per the brief — but it is exactly the
  surface the **deferred, human-verified headline leg** rides on: aws-cli v2's default
  integrity protections drive checksum members into `CompletedPart`, and an SDK that populates
  one will see the entire `aws s3 cp` of an 8 GB file fail at Complete rather than at the part.
  Worth explicitly exercising when the large-object leg is confirmed by hand (SUMMARY §9), and
  worth a decision on whether "tolerate and ignore, but do not claim to have verified" is the
  better posture.
- **NEEDS-HUMAN [impl] — a doc comment misstates the derivation cut by 2×.**
  `crates/core/src/multipart.rs:3405` says the `Parts` arm "stops enumerating at
  `DRAIN_BATCHES_PER_PASS × B_OPS / 2` = 4,000 units"; the code at `:2762` cuts at
  `DRAIN_BATCHES_PER_PASS * B_OPS / 4` = **2,000**. The regression test still straddles the
  real boundary (4,001 > 2,000), so nothing is broken — but this is the comment a future
  reader will use to decide whether a new population still exercises the truncation path.
- **NEEDS-HUMAN — `max_sessions()` derives to **74**.** Measured off the live `mpuctl` record:
  `{"max_sessions":74}` (`crates/core/src/multipart.rs:191`, `W_REF / u_ref()`). Seventy-four
  concurrent staged-reference-holding sessions is the whole deployment's ceiling; past it every
  `CreateMultipartUpload` is `503 SlowDown` until one drains. That is faithfully derived from
  0016's worked example and so is "per spec", but it is a fitness-to-purpose number the human
  should see stated in units of workload before sign-off, not only as a formula.

## Scope notes

- The four iteration-6 carry-forwards are genuinely fixed and I verified each in the source:
  UploadPartCopy refused before the UploadPart arm (`crates/gateway-s3/src/lib.rs:1964`), the
  drain fault split out as `MultipartFault::PlacementDraining` → `503 SlowDown`
  (`crates/server/src/multipart.rs:465-475`, `crates/gateway-s3/src/lib.rs:2341`), the
  same-part-number retry re-examined rather than 404'd
  (`crates/server/src/multipart.rs:556-608`), and the stale `MAX_STAGED_CHUNKS` comment
  corrected (`crates/core/src/multipart.rs:121-124`).
- I did **not** re-raise the `Completed`-tombstone / `max_records` exhaustion or the reaper's
  window-driven exits: both are deferred to #625, and the rubric's *Deferrals are settled*
  rule binds me.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Decide whether executor ownership can be required or injected so the runtime capability probe is unnecessary — its silent no-runtime path moves inline retirement entirely to #625 (`crates/server/src/multipart.rs:164`); additionally, `scripts/mutants-in-diff` was unavailable, so only the 925-mutant enumeration, not the reported outcomes, was reproduced.
- [ ] T1 Structure — Maintainers must decide whether the planned 44-file, 14,117-line cross-plane slice remains one reviewable unit or re-enters Plan for splitting — persisted formats, gateway behavior, and destructive maintenance now change together, increasing the chance that lifecycle coupling escapes review.
- [ ] T3 Runtime — A human must validate a stock-client ≥8 GiB round-trip on the deployed topology — the AWS CLI, live endpoint, and required disk topology were absent, so evidence is in-process only; with `S3_ENDPOINT` and `BUCKET` set, run `p="${PDCA_SCRATCH:-${TMPDIR:-/tmp}}/pdca-reviewer-508-8g"; truncate -s 8G "$p.in"; aws --endpoint-url "$S3_ENDPOINT" s3 cp "$p.in" "s3://$BUCKET/pdca-508-8g"; aws --endpoint-url "$S3_ENDPOINT" s3 cp "s3://$BUCKET/pdca-508-8g" "$p.out"; cmp "$p.in" "$p.out"; rm -f "$p.in" "$p.out"` and require `cmp` success.
- [ ] T4 Contribution — A human must reconcile the unavailable `scripts/review-branch` batch before relying on its reported 32 findings — independently, all 44 affected paths had no non-main history and 252 closed PRs yielded no rejected overlap, so prior-art novelty is mechanically clear but the rubric finding set is not.
- [ ] T5 Judgment — The architecture board must reconcile durable group identity and enforce the one-merge release with #625/#633 — the draft requires an independent group nonce (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:508`) but later equates it to the upload id (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2318`), while releasing before the reaper violates its normative order (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:236`).
- [ ] Validation — fitness-to-purpose — Product sign-off must decide whether the integrated #508/#625/#633 stack satisfies real large-object and operator-lifecycle needs — automated small-topology success cannot establish deployed ≥8 GiB interoperability, recovery, or drain behavior.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo build --workspace --exclude wyrd-dst --all-targets` failed with exit status: 101
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 32 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Re-enter Plan: this 7th attempt is still one 44-file / 14,117-line cross-plane slice (T1 Structure, §6). Split the re-plan along the seams the brief's own Open question 1 already names, not per S3 operation: (i) the `scan_page` trait seam + backends + conformance suite; (ii) decision-7 segmentation and the `chunk_map` shape change; (iii) the custodian-side reference-set / GC changes of decision 2; (iv) the gateway verbs and wire surface. Each seam becomes its own reviewable tracker slice/issue rather than one monolithic patch. Carry forward into the new slices' briefs, so the next Do pass isn't blind: - Adversary-found, not yet in any brief: CreateMultipartUpload answers false 503 SlowDown under ordinary client concurrency (aws-cli default 10 concurrent) on an empty store — the CAS-contention retry needs separating from the id-collision retry with a real bound (crates/server/src/multipart.rs:46,297,320-322). - The operator-visible startup signal (brief Scope item) is wired to `Gateway::with_reaper`, which has zero callers anywhere in the tree — functionally dead in every real deployment (crates/server/src/lib.rs:201; crates/server/src/cli.rs:1588). - GC's orphan-ledger walk lost its SCAN_CAP heap bound exactly where this patch's own arithmetic makes the population large — needs a bounded page walk, a design call for whichever slice owns GC (crates/custodian/src/gc.rs:720-738). - Routing regression: GET/HEAD ?partNumber=N (a valid S3 part-ranged GetObject) now wrongly answers 400 instead of the prior 501 (crates/gateway-s3/src/lib.rs:629,1955). - C4 xtask ci gate fail looks like a stale `target/` cache artifact (mtimes predate sources), not a real break — re-run `cargo clean && xtask ci` before treating any future C4 red here as genuine.
- By / date: Eduard Ralph / 2026-07-25

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
