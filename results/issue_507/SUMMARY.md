# Result — issue 507 / list-objects-v2

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: ListObjectsV2 (`GET /bucket?list-type=2`) with `prefix`, `delimiter`
- Success criterion: against the in-process loopback S3 gateway with objects stored
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: route bucket-scoped GETs (relax the object-path guard for GET on `/bucket`),

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue 507: implement S3 ListObjectsV2 and v1 bucket listings, including URL encoding, delimiter-aware pagination, and bucket/subresource routing.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is falsifiable at the signed HTTP/SDK boundary and distinguishes absent from empty buckets, pagination/resume semantics, URL projection, and error bodies; the corresponding wire assertions are grounded at `crates/server/tests/s3_list_objects.rs:819`. |
| C2 Reproduction (red pre-fix) | PASS | Base `07d0244` plus only the added wire test compiled and ran independently, with 23/24 assertions red on the base bucket-GET `400 InvalidRequest` behavior; the non-vacuous invalid-encoding body discriminator is at `crates/server/tests/s3_list_objects.rs:935`. |
| C3 Change | PASS | The change stays within the stated six-file seam/routing/render/test surface, and the protocol-neutral container seam preserves the absent-versus-empty distinction at `crates/gateway-core/src/lib.rs:227`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the verification with the host caveat, or rerun `cargo deny check` with a writable Cargo advisory database — direct red→green is confirmed (23 failures to 24 passes), and native CI passed through tests/machete, but the final deny scan could not acquire read-only `/home/eddie/.cargo/advisory-dbs/db.lock`; listing coverage is grounded at `crates/server/tests/s3_list_objects.rs:819`. |
| C5 Causal adequacy | PASS | The implementation removes the bucket-route rejection and performs raw-key grouping/resume before render-time projection rather than adding a capability probe or symptom guard; the v1/v2 resume distinction is grounded at `crates/gateway-s3/src/lib.rs:587`. |
| T1 Structure | PASS | The human need not accept S3 vocabulary leaking into core: the seam exposes neutral listed-object data while grouping and pagination remain in the S3 layer at `crates/gateway-core/src/lib.rs:227` and `crates/gateway-s3/src/lib.rs:787`. |
| T2 Shape | PASS | The wire contract has exact assertions for encoded keys/echoes, opaque-token resume, and `InvalidArgument`, so response-shape compatibility is directly constrained at `crates/server/tests/s3_list_objects.rs:849` and `crates/server/tests/s3_list_objects.rs:890`. |
| T3 Runtime | PASS | The in-process loopback gateway exercised signed SDK/raw-HTTP requests successfully (24/24), including token precedence and common-prefix resume behavior at `crates/server/tests/s3_list_objects.rs:1002` and `crates/server/tests/s3_list_objects.rs:1042`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether closed/rejected work contains conflicting prior art before contribution — merged history was checked by every affected file path and showed only existing S3/object work, but the supplied artifacts provide no mechanically authoritative closed/rejected-work index, so duplicate/conflict risk is not fully discharged. |
| T5 Judgment | PASS | The bounded full-scan/sequential-read Alpha tradeoff is explicit at `crates/gateway-core/src/lib.rs:211`, while correctness-sensitive pagination remains covered end to end; no additional contested root-cause or scope decision emerged. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether stock-client browsing is fit for release — in-process SDK/raw-wire behavior is green, but sign-off should run the brief's off-Check `aws s3 ls` and `aws s3 sync` round-trip because those named client workflows determine user-facing fitness. |

### Advisory — adversary

# check-advisory-adversary.md — issue 507 / list-objects-v2 (iteration 5)

Verdict: **could not refute the fix.** Both legs of the red→green proof were re-run
independently, and seven adversarial probes against edges the suite does not cover all
passed. Findings below are nits and one forward hazard, none refuting causal adequacy.

## Evidence re-verified (not taken on trust)

- The worktree diff at `$PDCA_TARGET` matches `patch.diff` byte-for-byte (identical
  `git patch-id`, base 07d0244 as the brief claims). Green leg re-run in place:
  `cargo test --test s3_list_objects` → **24/24 pass**. Red leg independently reproduced in
  a scratch clone at 07d0244 with ONLY the test file added: **23/24 fail by assertion**
  (400 `InvalidRequest` on every bucket GET — exactly the failure mode the brief's
  Falsifiability section predicts), no compile error. The test drives the production path
  (real loopback listener, stock `aws-sdk-s3` client + raw SigV4-signed HTTP); no parallel
  re-implementation, no mocks.
- Refutation attempts that FAILED (probes run against the patched build): prefix not
  delimiter-terminated (`prefix=a&delimiter=/` over `{a/1,a/2,ab,b}` → `CP=[a/]`,
  `Contents=[ab]`); a folder-marker object literally named `a/` rolls into its own CP and
  pages exactly once under `max-keys=1` token chaining; v2 `start-after=a/` while an object
  `a/` exists still emits the rollup; an exact-fit page (`max-keys` == item count) is NOT
  truncated and carries no token; an EMPTY `continuation-token=` answers 400
  `InvalidArgument` (empty string is rejected by `base64_decode`,
  `crates/gateway-s3/src/checksum.rs:180`); `max-keys=abc` answers 400 with the
  `<Code>InvalidArgument</Code>` body; a v1 listing under `encoding-type=url` does encode
  `<Key>` (`a&b/c d` → `a%26b/c%20d`). All 7 probes green.

## Findings

- NEEDS-HUMAN [impl] — weak-test conformance gap, behavior itself verified correct: the
  brief mandates the encoded-key oracle "(v2 and v1)" and "MUST assert EVERY encoded
  response element", but the v1 encoding test
  (`crates/server/tests/s3_list_objects.rs:953`) builds pages containing ONLY
  CommonPrefixes, so no encoded v1 `<Key>` is ever asserted anywhere in the suite. My probe
  confirms production is correct (v1 shares `render_contents`,
  `crates/gateway-s3/src/lib.rs:875`), so this is a one-assert test addition, not a code
  fix.
- NEEDS-HUMAN — forward hazard for #511 (cross-issue scope, not exploitable in this diff):
  the listing route percent-decodes the bucket segment
  (`crates/gateway-s3/src/lib.rs:1482`) and `list_container` composes the scan prefix by
  string interpolation (`crates/server/src/lib.rs:480`), so `GET /a%2Fb?list-type=2` scans
  `dirent:0/a/b/` — bucket `a`'s `b/…` subtree. Today this always 404s (`bucket:a/b`
  marker can never exist — no production writer), but #511's CreateBucket MUST reject
  bucket names containing `/` (and S3-invalid names generally) or a marker for `a/b` would
  let one "bucket" read another's keyspace. Deserves a note on #511.
- `list_v2_double_slash_bucket_path_is_rejected_not_listed`
  (`crates/server/tests/s3_list_objects.rs:1091`) is vacuously green on the C4-verify red
  leg (the base answers 400 with no `<ListBucketResult>`, satisfying both assertions — the
  single "pass" in my 23/24 red run). It discriminates only against the v4
  `trim_start_matches` defect it locks, which is its stated job; the red evidence is
  carried by the other 23 tests. No action needed — recorded so nobody counts it as red
  evidence.
- Minor, deliberate brief deviation (advisory only): the brief says "Other bucket-scoped
  methods keep today's behaviour", but the denylist runs before the method match
  (`crates/gateway-s3/src/lib.rs:1472-1489`), so e.g. `PUT /bucket?acl` /
  `POST /bucket?delete` now answer 501 `NotImplemented` where the base answered 400
  `InvalidRequest`. Fail-closed and arguably safer (a future #511/509 method extension
  cannot silently mishandle a subresource), but it is a behavior change the brief's
  letter did not order.
- Minor conformance nit (unpressed): `max-keys=+5` is accepted as 5 — Rust's
  `parse::<usize>` accepts a leading `+` (`crates/gateway-s3/src/lib.rs:703-712`); AWS
  likely rejects the form. No stock SDK emits it; not worth an iteration on its own.

## Reviewer-verdict check

`check-gates.json` claims C4-ci pass and C4-verify "red without the fix, green with it" —
both independently corroborated above; I found no rationalized claim. Attempted to refute
the evidence, the pagination/rollup logic, the encoding projection, and the routing split;
could not.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept the verification with the host caveat, or rerun `cargo deny check` with a writable Cargo advisory database — direct red→green is confirmed (23 failures to 24 passes), and native CI passed through tests/machete, but the final deny scan could not acquire read-only `/home/eddie/.cargo/advisory-dbs/db.lock`; listing coverage is grounded at `crates/server/tests/s3_list_objects.rs:819`.
- [x] T4 Contribution — Decide whether closed/rejected work contains conflicting prior art before contribution — merged history was checked by every affected file path and showed only existing S3/object work, but the supplied artifacts provide no mechanically authoritative closed/rejected-work index, so duplicate/conflict risk is not fully discharged.
- [x] Validation — fitness-to-purpose — Decide whether stock-client browsing is fit for release — in-process SDK/raw-wire behavior is green, but sign-off should run the brief's off-Check `aws s3 ls` and `aws s3 sync` round-trip because those named client workflows determine user-facing fitness.
- [x] weak-test conformance gap, behavior itself verified correct: the
- [x] forward hazard for #511 (cross-issue scope, not exploitable in this diff):

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
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- issue_507: forward to #511 — v1 encoding weak-test gap (no encoded v1 `<Key>` asserted; behavior verified correct by adversary probe) — add the one-assert v1 encoded-Key test there.
- issue_507: plan into #511 — off-Check `aws s3 ls` / `aws s3 sync` round-trip acceptance (deferred until #511 lands `bucket:{name}` marker writes; doctor row "aws cli (S3 gateway round-trip)").
- issue_507: plan into #511 — CreateBucket MUST reject bucket names containing `/` (and S3-invalid names generally), else a marker for `a/b` lets one "bucket" read another's keyspace via the listing scan-prefix interpolation.
