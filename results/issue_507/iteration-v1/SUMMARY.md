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

Task under review: implement S3 ListObjectsV2 and a ListObjects v1 compatibility path with filtering, delimiter grouping, pagination, bucket-existence semantics, and wire-level XML responses.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is falsifiable at the signed HTTP/SDK boundary and distinguishes absent from marker-backed empty buckets; the human-impacting live-marker sequencing decision is isolated under T5/Validation. |
| C2 Reproduction (red pre-fix) | PASS | In a scratch clone at base `07d0244` with only the added test retained, `cargo test -p wyrd-server --test s3_list_objects` compiled and failed 9/9 assertions at the former bucket-only rejection, grounding the red symptom at `crates/server/tests/s3_list_objects.rs:138`. |
| C3 Change | FAIL | `max-keys=0` can produce `IsTruncated=true` while returning no resume token, leaving a conforming paginator unable to advance; the zero-budget branch is at `crates/gateway-s3/src/lib.rs:471`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether focused red→green plus formatting is sufficient without the project CI wrapper — the wrapper named in `check-gates.json` is absent from the target, the focused target suite passed 9/9 at `crates/server/tests/s3_list_objects.rs:138`, and substitute all-feature clippy stopped on unrelated existing warnings. |
| C5 Causal adequacy | PASS | The former bucket-only dispatch rejection is removed at its cause by routing bucket-scoped GET before the object-path guard, rather than by a capability probe or fallback guard (`crates/gateway-s3/src/lib.rs:1165`). |
| T1 Structure | PASS | Protocol-neutral container enumeration remains in the core seam/server implementation while S3 grouping and token semantics remain in the wire crate (`crates/server/src/lib.rs:458`). |
| T2 Shape | PASS | The stock SDK drives the shipping loopback HTTP surface, and the assertions cover ordering, metadata, prefix, delimiter, pagination, errors, escaping, empty buckets, and v1 shape (`crates/server/tests/s3_list_objects.rs:138`). |
| T3 Runtime | FAIL | A nonempty listing requested with `max-keys=0` sets truncation before consuming any key, then suppresses `next_key`, violating the response invariant that a truncated page can be resumed (`crates/gateway-s3/src/lib.rs:471`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether overlapping closed/rejected work exists — merged history was checked by all six affected paths, but closed/rejected review history could not be mechanically settled by affected file path, so duplicate or superseded intent remains possible. |
| T5 Judgment | NEEDS-HUMAN | Decide whether to ship listing before marker creation/backfill is available — production listing returns `NoSuchBucket` whenever the external #511/backfill prerequisite has not populated the marker read at `crates/server/src/lib.rs:467`, directly limiting live usefulness. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether real-client usefulness is acceptable before sign-off — run `aws s3 ls s3://<marker-backed-bucket> --endpoint-url http://<gateway>` and `aws s3 sync <dir> s3://<marker-backed-bucket> --endpoint-url http://<gateway>`, confirming complete stable listings and no pagination loop; the in-process SDK passed, but AWS CLI and live marker topology were not exercised. |

### Advisory — adversary

# Adversarial review — issue 507 / list-objects-v2

Evidence re-run: green leg re-executed in-process at `$PDCA_TARGET` — all 9 tests in
`crates/server/tests/s3_list_objects.rs` pass against the patched production build. The red
leg was not re-executed (it requires mutating the read-only worktree); instead verified
statically that the test imports no post-patch production symbol (raw-bytes marker seeding,
wire-only assertions), so on the wave base every test fails by assertion at the 400
`InvalidRequest` — consistent with the C4-verify PASS. Findings below.

- NEEDS-HUMAN [impl] — `max-keys=0` diverges from S3 and can wedge clients: the clamp
  (`crates/gateway-s3/src/lib.rs:529`) admits `0`, and `compute_page` then trips the budget
  check before consuming anything (`lib.rs:471-473`), so `next_key` is never set and
  `lib.rs:502` emits **no** `NextContinuationToken` while `IsTruncated=true`. Concrete
  failing case: `GET /bucket?list-type=2&max-keys=0` on a non-empty bucket returns
  `IsTruncated=true`, `KeyCount=0`, no token — real S3 returns `IsTruncated=false`. A client
  that re-sends while `IsTruncated` (no token to advance) loops forever on an identical
  request. Untested by the suite.

- NEEDS-HUMAN [impl] — the subresource fence on the new bucket route is bypassable by
  percent-encoding: `unsupported_subresource` matches **raw** query keys
  (`crates/gateway-s3/src/lib.rs:363-367`), so `GET /bucket?%61cl` (or `?upload%73`) skips
  the 501 guard at `lib.rs:1173` and is answered with a listing document — precisely the
  "bucket subresource op silently answered with a listing" the routing decision forbids
  (comment at `lib.rs:1166-1172`), and it re-opens the encoded variant of the 508
  ListMultipartUploads collision the guard exists to prevent. Pre-existing raw matching was
  harmless while every bucket GET answered 400; this diff makes the bypass yield a 200.
  Fix: match the denylist against decoded keys on the bucket path.

- NEEDS-HUMAN [impl] — test gap on the centerpiece logic: the group-consume/resume path
  (`crates/gateway-s3/src/lib.rs:482-488` — a CommonPrefix group consumed atomically so the
  token skips it) is never exercised **under truncation**: the delimiter test
  (`crates/server/tests/s3_list_objects.rs:201`) uses no `max-keys`, and the pagination test
  (`s3_list_objects.rs:253`) uses no delimiter. I hand-traced delimiter=`/` + `max-keys=1`
  over `{a/1,a/2,b/1,b/2,c}` and it pages correctly (a/, b/, c — once each), so I could not
  refute the logic — but a regression that double-emits a CommonPrefix across pages, the
  exact codex finding the design cites, would pass this suite. One delimiter+max-keys
  chaining test closes it.

- NEEDS-HUMAN — silently-ignored v2 parameters vs the goal's named clients: `start-after`
  and `encoding-type` are neither implemented nor rejected — they are absent from the
  denylist and never parsed in `list_objects` (`crates/gateway-s3/src/lib.rs:511-539`), so
  they route to a plain listing that ignores them. The brief scopes both out, but "omit" was
  implemented as "silently accept and ignore": rclone/minio-go send `encoding-type=url` and
  URL-decode the returned keys, so a key literally named `a%2Fb` is corrupted to `a/b`
  client-side (the Goal names rclone); a `start-after` client re-receives keys it already
  consumed. Whether to 501/400 these forms until implemented (vs. the current silent
  ignore) is a scope/fitness call the brief did not settle — build-notes (withheld here)
  were supposed to state the aws-cli result, which the reviewer should have confirmed.

- NEEDS-HUMAN — the seam doc's "returning it whole adds no new cost class"
  (`crates/gateway-core/src/lib.rs:210`) is an unwarranted claim: `list_container` issues
  one **sequential** `read_inode` point-read per dirent (`crates/server/src/lib.rs:495`),
  bounded only by `SCAN_CAP = 1<<20` (`crates/traits/src/lib.rs:286`), and the entire
  scan + N inode reads + sort re-runs for **every page** of a paginated listing (the wire
  layer discards all but ≤1000 rows per request). On redb this is in-memory; on the
  tikv/fdb backends it is up to N serial network RTTs per page, ~N×pages per full `aws s3
  sync` enumeration. The brief blessed the materialized-Vec stance for the *scan*, not the
  N+1 inode reads; whether this is acceptable Alpha debt or needs batching before landing
  is an architectural call.

- Conformance nits (advisory, no adjudication needed): the v1 shim emits `<NextMarker>`
  even without `delimiter` (`crates/gateway-s3/src/lib.rs:727-729`) where AWS emits it only
  when a delimiter is present — a benign superset, but the test
  (`crates/server/tests/s3_list_objects.rs:392`) locks the non-AWS behavior in; and any
  `list-type` value other than `2` (e.g. `list-type=3`) silently falls to the v1 shim
  (`lib.rs:521`) where AWS answers 400 `InvalidArgument`.

Attempted and could not refute: empty `continuation-token=` silently restarting the listing
(refuted — `base64_decode` rejects the empty string, `crates/gateway-s3/src/checksum.rs:180`,
so it 400s as required); cross-bucket scan spill `foo` → `foobar` (fenced by the trailing
`/` in the scan prefix, `crates/server/src/lib.rs:480`); XML-escape tautology (the SDK's XML
parser is an independent oracle — an unescaped `&` fails the parse, and the special-key test
drives it); combined `Contents`+`CommonPrefixes` `max-keys` counting and cross-page
common-prefix dedup (hand-traced, correct); percent-encoded `+`/`/` in tokens surviving the
query round-trip (`query_param` treats `+` as literal, matching SigV4 canonical encoding).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether focused red→green plus formatting is sufficient without the project CI wrapper — the wrapper named in `check-gates.json` is absent from the target, the focused target suite passed 9/9 at `crates/server/tests/s3_list_objects.rs:138`, and substitute all-feature clippy stopped on unrelated existing warnings.
- [ ] T4 Contribution — Decide whether overlapping closed/rejected work exists — merged history was checked by all six affected paths, but closed/rejected review history could not be mechanically settled by affected file path, so duplicate or superseded intent remains possible.
- [ ] T5 Judgment — Decide whether to ship listing before marker creation/backfill is available — production listing returns `NoSuchBucket` whenever the external #511/backfill prerequisite has not populated the marker read at `crates/server/src/lib.rs:467`, directly limiting live usefulness.
- [ ] Validation — fitness-to-purpose — Decide whether real-client usefulness is acceptable before sign-off — run `aws s3 ls s3://<marker-backed-bucket> --endpoint-url http://<gateway>` and `aws s3 sync <dir> s3://<marker-backed-bucket> --endpoint-url http://<gateway>`, confirming complete stable listings and no pagination loop; the in-process SDK passed, but AWS CLI and live marker topology were not exercised.
- [ ] `max-keys=0` diverges from S3 and can wedge clients: the clamp
- [ ] the subresource fence on the new bucket route is bypassable by
- [ ] test gap on the centerpiece logic: the group-consume/resume path
- [ ] silently-ignored v2 parameters vs the goal's named clients: `start-after`
- [ ] the seam doc's "returning it whole adds no new cost class"

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
- Iteration delta (if iterating): Human rejects this attempt pending fixes for all advisory findings (check-review.md + adversary). Carry forward: - max-keys=0 must match S3: return IsTruncated=false with no keys, never a truncated page without a resume token (clamp at crates/gateway-s3/src/lib.rs:529 admits 0; zero-budget branch at lib.rs:471-473 suppresses next_key). Add a test for max-keys=0 on a non-empty bucket. - Subresource denylist on the bucket route must match percent-DECODED query keys: GET /bucket?%61cl (or ?upload%73) currently bypasses the 501 guard at lib.rs:1173 and receives a listing; decode before matching (unsupported_subresource, lib.rs:363-367). - Close the centerpiece test gap: add a delimiter + max-keys chaining test exercising the group-consume/resume path under truncation (lib.rs:482-488), asserting no CommonPrefix is double-emitted across pages. - Do not silently ignore start-after / encoding-type: reject the unimplemented forms (400/501) until implemented — rclone/minio-go send encoding-type=url and URL-decode returned keys, corrupting keys like a%2Fb; start-after clients re-receive consumed keys. - Correct or temper the seam doc claim at crates/gateway-core/src/lib.rs:210 ("no new cost class"): list_container does one sequential read_inode per dirent (crates/server/src/lib.rs:495) and re-runs scan+reads+sort per page; batch the inode reads or document the N+1 as explicit, bounded Alpha debt. - Conformance nits: v1 emits NextMarker without a delimiter (lib.rs:727-729, test locks non-AWS behavior in) and any list-type other than 2 silently falls to the v1 shim (lib.rs:521) where AWS answers 400 InvalidArgument — align with AWS or state the deviation deliberately.
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
