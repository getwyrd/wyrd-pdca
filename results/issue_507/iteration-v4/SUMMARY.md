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

Review of issue 507: implement signed S3 ListObjectsV2 and v1 bucket listings with filtering, delimiter grouping, pagination, bucket existence, and compatible error behavior.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is executable and distinguishes absent from empty buckets, exact sorted metadata, filtering/grouping, v1/v2 pagination, escaping, and invalid-token behavior; the wire assertions ground those outcomes at `crates/server/tests/s3_list_objects.rs:153`. |
| C2 Reproduction (red pre-fix) | PASS | In an attributable scratch clone, retaining the added wire test while reverting all production hunks produced 17/17 assertion failures because bucket-only GET still returned `400 InvalidRequest`; the routing boundary being exercised is `crates/gateway-s3/src/lib.rs:1311`. |
| C3 Change | PASS | The change stays within the declared metadata key, protocol-neutral listing seam, server scan/materialization, S3 routing/rendering, and wire-test surfaces; the bounded seam and its known N+1 cost are explicit at `crates/gateway-core/src/lib.rs:198`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the independently confirmed focused red→green despite the full `cargo xtask ci` rerun stopping only because `cargo deny` could not lock the read-only host advisory DB — all 17 listing tests pass green at `crates/server/tests/s3_list_objects.rs:253`, but the asserted complete gate remains provisional. |
| C5 Causal adequacy | PASS | The patch removes the bucket-only routing rejection and implements the missing listing path rather than adding a capability probe or symptom guard; dispatch now reaches the listing handler at `crates/gateway-s3/src/lib.rs:1318`. |
| T1 Structure | PASS | Protocol vocabulary and page computation remain in the S3 layer while storage returns neutral listed objects, preserving the intended boundary at `crates/gateway-core/src/lib.rs:198`. |
| T2 Shape | PASS | The implementation preserves the complete-or-error scan shape, sorts once per request, and counts contents plus common prefixes in one page algorithm; delimiter resume semantics are grounded at `crates/gateway-s3/src/lib.rs:531`. |
| T3 Runtime | PASS | The in-process signed SDK/HTTP suite passed 17/17, including sorted metadata, prefix/delimiter, red→green pagination, empty/missing buckets, invalid tokens, v1 markers, and inside-group resumes; the landed regression oracle is `crates/server/tests/s3_list_objects.rs:633`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether affected-path merged-history inspection is sufficient — history was checked for all six touched paths, but the supplied artifacts expose no mechanical closed/rejected-work index, so prior-art duplication outside merged history remains unsettled. |
| T5 Judgment | NEEDS-HUMAN | Decide whether Alpha may ship repeated scan + sequential N inode reads per page and listings that require a pre-existing bucket marker — this bounds latency and means live usability still depends on #511 or operator backfill, as documented at `crates/gateway-core/src/lib.rs:214` and `crates/server/src/lib.rs:463`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-process SDK wire coverage is representative of the named clients and deployment topology — run `aws s3 ls s3://<marker-backed-bucket> --endpoint-url http://<gateway>` and `aws s3 sync <dir> s3://<marker-backed-bucket> --endpoint-url http://<gateway>`, confirming nested keys list once and sync completes, because real AWS CLI acceptance was not exercised. |

### Advisory — adversary

# check-advisory-adversary.md — issue 507 / list-objects-v2 (iteration 4)

Independent re-runs (not taken on faith): **green leg** — all 17 tests in
`crates/server/tests/s3_list_objects.rs` pass against the patched worktree; **red leg** —
reproduced in a scratch clone at wave base `07d0244` with ONLY the test file added: it
compiles (no new production symbol) and all 17 tests fail **by assertion** (400
`InvalidRequest`), corroborating C4-verify. The iteration-3 refutation (resume inside a
delimiter group) is genuinely fixed and locked by two wire tests. Remaining refutations:

- NEEDS-HUMAN — **The headline Goal is still not met for the most common clients, and the
  in-code justification is factually wrong.** `crates/gateway-s3/src/lib.rs:634` claims
  "Stock `aws-cli` `ls`/`sync` do not send it [`encoding-type`]" — empirically false: I ran
  stock `aws-cli/2.36.1` (`aws s3 ls s3://bucket --debug`) and botocore **auto-injects**
  `encoding-type=url` into every ListObjects/V2 request (`encoding_type_auto_set: True`;
  observed wire query `?list-type=2&prefix=&delimiter=%2F&encoding-type=url`). The 501
  rejection at `crates/gateway-s3/src/lib.rs:635-641` therefore refuses **every** stock
  aws-cli / boto3 listing (and rclone, per the carry-forward's own note) — so the brief's
  Goal ("Unblocks `aws s3 ls`, `aws s3 sync`, rclone/restic/s3fs browsing") is unmet for
  those clients; only non-botocore SDKs (aws-sdk-rust/go/js) can list. The builder followed
  the brief's letter (the brief scopes `encoding-type=url` OUT, and the iteration-1
  carry-forward ordered loud rejection over silent corruption — 501 is the right *interim*
  behaviour), but scope and Goal now contradict: a human must decide whether to pull
  `encoding-type=url` into scope (URL-encode Key/Prefix/CommonPrefixes/Delimiter + emit
  `<EncodingType>` — modest) or accept that the off-Check acceptance (`aws s3 ls` doctor
  row) cannot pass. The reviewer/gates could not see this: the SDK-driven test suite never
  sends `encoding-type`, so C4 green does not exercise the client population the Goal names.

- NEEDS-HUMAN [impl] — **A v2 `start-after` exactly equal to a common prefix silently hides
  the whole group — the un-fixed residue of the iteration-3 refutation.** The group-skip
  predicate `crates/gateway-s3/src/lib.rs:556-557` (`r == cp.as_str() || r >= last_raw`)
  applies the `r == cp` collapse to **client-chosen** v2 `start-after` too. Reproduced on
  the patched code over the wire: bucket `{a/1, a/2, b}`, `?list-type=2&delimiter=/&
  start-after=a/` → `Contents=[b]`, `CommonPrefixes=[]` — `a/1`,`a/2` invisible. Under the
  raw-keyspace rule the iteration-3 adjudication itself established (and AWS's documented
  StartAfter semantics: "starts listing after this specified key"), `a/1 > "a/"` survives
  and AWS returns `CommonPrefixes=[a/]`. The `r == cp` clause is needed only for the
  **server-issued v1 `NextMarker`** resume (locked by the test at
  `crates/server/tests/s3_list_objects.rs:414-471`); for v2 it is dead weight — a
  server-issued v2 token always satisfies `r >= last_raw` (for a single-key group `{a/}`,
  `last_raw == cp`), so restricting the `r == cp` collapse to the v1 marker path fixes the
  case without breaking the v1 resume test. Realistic trigger: a folder-marker workflow
  (`start-after=<prefix>` to skip a zero-byte `a/` marker object).

- `crates/gateway-s3/src/lib.rs:803-843` (`render_list_v2`) omits the `<StartAfter>` echo
  AWS emits when the request carried `start-after` — minor conformance nit, tolerated by
  the SDK (optional field); note only, not pressed.

Attempted and could NOT refute: red→green validity (re-run both legs independently, above);
tautology/mocking (the test drives the shipping HTTP surface with a stock SDK, ETags
asserted against an independent SHA-256); pagination exactly-once under delimiter+max-keys=1
(test `s3_list_objects.rs:420-471` chains the group-consume/resume path); max-keys=0;
empty/absent-bucket 404-vs-200; empty or malformed continuation token (`base64_decode`
rejects `""` and non-canonical padding → 400, `crates/gateway-s3/src/checksum.rs:178-208`);
percent-encoded subresource bypass on the bucket route (decoded denylist,
`crates/gateway-s3/src/lib.rs:1327`); auth ordering (SigV4 verified at `lib.rs:1276` before
the new bucket dispatch at `lib.rs:1318`); cross-bucket scan spill (trailing-`/` fence in
`crates/server/src/lib.rs:478-481`); the iteration-2 findings (denylist additions incl.
`versioning`, v1 NextMarker value = common prefix) — all hold.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept the independently confirmed focused red→green despite the full `cargo xtask ci` rerun stopping only because `cargo deny` could not lock the read-only host advisory DB — all 17 listing tests pass green at `crates/server/tests/s3_list_objects.rs:253`, but the asserted complete gate remains provisional.
- [ ] T4 Contribution — Decide whether affected-path merged-history inspection is sufficient — history was checked for all six touched paths, but the supplied artifacts expose no mechanical closed/rejected-work index, so prior-art duplication outside merged history remains unsettled.
- [ ] T5 Judgment — Decide whether Alpha may ship repeated scan + sequential N inode reads per page and listings that require a pre-existing bucket marker — this bounds latency and means live usability still depends on #511 or operator backfill, as documented at `crates/gateway-core/src/lib.rs:214` and `crates/server/src/lib.rs:463`.
- [ ] Validation — fitness-to-purpose — Decide whether the in-process SDK wire coverage is representative of the named clients and deployment topology — run `aws s3 ls s3://<marker-backed-bucket> --endpoint-url http://<gateway>` and `aws s3 sync <dir> s3://<marker-backed-bucket> --endpoint-url http://<gateway>`, confirming nested keys list once and sync completes, because real AWS CLI acceptance was not exercised.
- [ ] **The headline Goal is still not met for the most common clients, and the
- [ ] **A v2 `start-after` exactly equal to a common prefix silently hides
- [ ] external dependency. The off-Check `aws s3 ls`/`sync` acceptance (which needs a

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Rejected on the adversary's two landed findings; both verified as covered by NO other tracker issue (repo-wide search: zero hits for encoding-type; start-after hits unrelated), so neither can be punted — and epic #513's Alpha client bar (stock aws-cli + boto3 list working) plus #507's own tracker acceptance ("aws s3 ls / aws s3 sync work") make item 1 part of THIS issue's goal. The brief must change, hence iterate-plan: 1. Scope/Goal contradiction — pull `encoding-type=url` INTO scope. The brief scopes it out while its Goal names aws-cli/rclone; empirically (aws-cli 2.36.1 --debug) botocore auto-injects `encoding-type=url` into every ListObjects/V2 request, so the current 501 rejection (crates/gateway-s3/src/lib.rs:635-641) refuses every stock aws-cli / boto3 / rclone listing, and the in-code comment at lib.rs:634 claiming aws-cli does not send it is factually wrong. Fix is modest per the adversary: URL-encode Key/Prefix/CommonPrefixes/Delimiter (and StartAfter/Marker echoes) when encoding-type=url is requested and emit `<EncodingType>url</EncodingType>`. Add a wire test that sends encoding-type=url (the SDK suite never does — that is why C4 green missed this) and asserts a key like `a&b/c d` round-trips URL-encoded. Note #512's aws-cli/boto3 harness will deterministically re-expose this if skipped. 2. v2 `start-after` equal to a common prefix silently hides the whole group (residue of the iteration-3 refutation). Reproduced on the wire: bucket {a/1, a/2, b}, `?list-type=2&delimiter=/&start-after=a/` → Contents=[b], CommonPrefixes=[] — AWS returns CommonPrefixes=[a/]. Cause: the group-skip predicate at crates/gateway-s3/src/lib.rs:556-557 applies the `r == cp` collapse to client-chosen v2 start-after; that collapse is only valid for the server-issued v1 NextMarker resume. Fix: restrict the `r == cp` clause to the v1 marker path (a server-issued v2 token always satisfies `r >= last_raw`), keeping the v1 resume test at crates/server/tests/s3_list_objects.rs:414-471 green. Add a wire test for v2 start-after exactly equal to a common prefix (folder-marker workflow trigger). Minor, while in there: emit the `<StartAfter>` echo in render_list_v2 when the request carried start-after (noted, not pressed). Mainline is otherwise sound — red→green independently corroborated both legs (17/17), the iteration-3 inside-group resume fix holds, denylist/token/auth probes unrefuted. Keep the approach; amend the brief's scope and fix the two items above.
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
