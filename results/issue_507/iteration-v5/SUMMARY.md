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

Review of issue 507: implement bucket-scoped S3 ListObjectsV2 and v1 listing, including pagination, delimiter rollups, and `encoding-type=url` compatibility.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The decision boundary is explicit and falsifiable: bucket listing must distinguish absent from empty buckets and preserve raw-key resume semantics while projecting URL encoding only on response values; the implementation entry point is grounded at `crates/gateway-s3/src/lib.rs:645`. |
| C2 Reproduction (red pre-fix) | PASS | Independently running the added wire suite against base production produced the specified `400 InvalidRequest` behavior and 0/23 passing tests; the non-vacuous error-code oracle is at `crates/server/tests/s3_list_objects.rs:934`. |
| C3 Change | PASS | The six-file change stays within the listing route, neutral gateway seam, metadata key helper, token codec, server implementation, and its wire tests; the resume-kind boundary that prevents v1 semantics leaking into v2 is at `crates/gateway-s3/src/lib.rs:497`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the otherwise reproduced gate with `cargo-deny` outstanding — red→green was independently 0/23→23/23 and typos/docs/fmt/clippy/build/workspace-tests/machete passed, but the scanner first hit a read-only advisory lock and its scratch-state retry did not complete, so the full asserted CI row was not independently reproduced. |
| C5 Causal adequacy | PASS | The change removes the bucket-route rejection and separates v1-marker from v2-resume comparison at the pagination cause (`crates/gateway-s3/src/lib.rs:579`); it adds no capability probe or fallback guard matching the symptom-guard smell test. |
| T1 Structure | PASS | The architectural decision is preserved: storage exposes a protocol-neutral complete sorted container view, while S3 grouping/paging remains wire-side (`crates/gateway-core/src/lib.rs:227`). |
| T2 Shape | PASS | Exact wire-shape coverage exercises encoded keys, prefix, delimiter, common prefixes, start-after, marker/next-marker, untouched tokens, and `InvalidArgument`; the raw-wire oracle begins at `crates/server/tests/s3_list_objects.rs:818`. |
| T3 Runtime | PASS | The in-process loopback gateway exercised the patched runtime successfully with all 23 listing tests passing, including exact-common-prefix resume at `crates/server/tests/s3_list_objects.rs:1001` and token precedence at `crates/server/tests/s3_list_objects.rs:1041`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether closed/rejected work contains overlapping changes before contribution sign-off — merged history was checked by all six affected paths, but GitHub's closed-PR search did not mechanically establish affected-file overlap for rejected work. |
| T5 Judgment | PASS | The remaining tradeoff is explicit rather than hidden: listing materializes and sorts a bounded view and performs sequential inode reads, while correctness-critical raw-key paging is centralized at `crates/gateway-s3/src/lib.rs:511`; no new ambiguous scope was found. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether stock-client interoperability is acceptable — the Rust SDK and raw signed HTTP passed, but the named aws-cli/boto3 workflow was not exercised; seed a bucket marker, run `aws --endpoint-url http://HOST:PORT s3 ls s3://BUCKET` and a two-way `aws s3 sync`, and confirm encoded names plus multi-page listings complete without duplicates. |

### Advisory — adversary

# check-advisory-adversary.md — issue 507 / list-objects-v2 (iteration 5)

Skeptic's pass. I re-ran the red→green proof independently (not just re-read the gate) and
attacked the fix with live wire probes against a patched build. Verdict up front: **the
evidence is genuine and I could not refute the mainline**; two minor findings below.

## Evidence re-run (independent, not the gate's word)

- **Green leg re-run**: `cargo test -p wyrd-server --test s3_list_objects` on the target
  worktree → **23/23 pass** (0.09s, real loopback TCP + SigV4 + stock `aws-sdk-s3`).
- **Red leg re-run**: fresh clone at the brief's base `07d0244` + ONLY the added test file
  (`crates/server/tests/s3_list_objects.rs`) → **compiles clean, 23/23 fail by assertion**
  (every failure is the base's `400 InvalidRequest` on bucket GET — no compile-error
  masking, no vacuous status-only pass: the `encoding-type=broken` test failed on the
  `<Code>InvalidArgument</Code>` body as the brief demanded, and
  `get_bucket_versioning_is_501_not_a_listing_document` failed on the body code too).
- **Production path**: the tests drive the shipping dispatcher through a real listener with
  signed requests; the only test-side shortcut is the raw `bucket:{name}` marker seed, which
  is exactly what the brief mandates (#511 owns the writer). No parallel re-implementation,
  no mocked-away defect found.
- **Patch = worktree**: `git apply --numstat patch.diff` matches `git diff --numstat` on
  `$PDCA_TARGET` file-for-file, line-for-line.

## Refutation attempts against the fix (live probes, patched build)

Attempted and **failed** to refute:

- v2 `start-after` equal to a common prefix **with a real folder-marker object `a/` present**
  (`{a/, a/1, b}`, `start-after=a/`): returns `CommonPrefixes=[a/]`, `Contents=[b]` — the
  `a/` key itself correctly excluded (`>` not `>=`), rollup kept. Delta 2 holds beyond the
  suite's own case (`crates/gateway-s3/src/lib.rs:464-479`).
- empty `continuation-token=` → `400 InvalidArgument` (not a silent restart).
- token decoding to a key past the end of the bucket → empty untruncated 200, no phantom token.
- `max-keys=5000` → clamps to 1000; `max-keys=` 23-digit overflow → `400 InvalidArgument`
  (`lib.rs:580-593`), never a panic.
- percent-encoded slash in the bucket path (`GET /bucket%2Fa?list-type=2`) → `404
  NoSuchBucket`; the decoded name cannot alias into bucket `bucket`'s `a/` dirent subtree
  because the marker read gates first (`crates/server/src/lib.rs:462-470`).
- v1 truncated listing WITHOUT a delimiter → `IsTruncated=true`, no `<NextMarker>` (AWS shape).
- duplicate `list-type=2&list-type=3` → first occurrence wins, deterministic 200.
- token-vs-start-after precedence, opaque-token opacity (the `=`-padding case), decoded-key
  subresource fence (`?%61cl`), XML-special keys, `NoSuchBucket`/empty-bucket split — all
  covered by the suite itself and re-verified green.

## Findings

- NEEDS-HUMAN [impl] — `bucket_scoped_path` dead arm + factually wrong comment
  (`crates/gateway-s3/src/lib.rs:427-436`): `trim_start_matches('/')` strips ALL leading
  slashes, so the `Some(_) => None` "empty bucket segment (`//…`) → neither" arm is
  unreachable and its comment is false — probed live: a signed `GET //bucket?list-type=2`
  answers a **200 listing** (the base answered 400; AWS treats an empty leading path segment
  as an error). Mitigation: this mirrors the pre-existing laxity in `split_bucket_key`
  (`lib.rs:413`, `//bucket/key` was already accepted as an object path), and the request is
  fully authenticated for a bucket the caller could list canonically anyway — a conformance
  nit and a wrong comment, not a disclosure. Fix is a one-liner (single `strip_prefix('/')`
  or delete the dead arm and correct the comment).
- (informational, no prefix) The brief's `encoding-type` oracle says "`a&b/c d` returns as
  `a%26b/c%20d` **(v2 and v1)**"; the v1 wire test
  (`crates/server/tests/s3_list_objects.rs`, `list_v1_encoding_type_url_encodes_marker_and_next_marker`)
  asserts encoded `Marker`/`NextMarker`/`CommonPrefixes` but never an encoded v1 `<Key>`.
  Risk ≈ nil — `render_contents` (`lib.rs:752-766`) is shared verbatim by both renderers —
  but a one-line `<Key>` assertion on the v1 leg would close the letter of the criterion.
- (informational, no prefix) The `SCAN_CAP`-exceeded → 500-class path
  (`crates/server/src/lib.rs:466` via `scan`'s complete-or-`Err` contract,
  `crates/traits/src/lib.rs:286`) is asserted by no test — seeding 2^20 objects is
  impractical in-process; the mapping rides the pre-existing `classify` of
  `ScanCapExceeded` as Terminal. Acceptable as designed; noting so no one believes it tested.

## Reviewer-verdict audit

`check-gates.json` claims nothing beyond the two C4 rows, both of which I reproduced
independently (red 23/23 by assertion at base `07d0244`, green 23/23 with the patch). No
unwarranted claim found. Attempted to refute the evidence, the Delta-1 encoding surface,
the Delta-2 resume semantics, the routing fence, and the token machinery; **could not** —
beyond the minor `bucket_scoped_path` nit above.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept the otherwise reproduced gate with `cargo-deny` outstanding — red→green was independently 0/23→23/23 and typos/docs/fmt/clippy/build/workspace-tests/machete passed, but the scanner first hit a read-only advisory lock and its scratch-state retry did not complete, so the full asserted CI row was not independently reproduced.
- [ ] T4 Contribution — Decide whether closed/rejected work contains overlapping changes before contribution sign-off — merged history was checked by all six affected paths, but GitHub's closed-PR search did not mechanically establish affected-file overlap for rejected work.
- [x] Validation — fitness-to-purpose — Decide whether stock-client interoperability is acceptable — the Rust SDK and raw signed HTTP passed, but the named aws-cli/boto3 workflow was not exercised; seed a bucket marker, run `aws --endpoint-url http://HOST:PORT s3 ls s3://BUCKET` and a two-way `aws s3 sync`, and confirm encoded names plus multi-page listings complete without duplicates.
- [ ] `bucket_scoped_path` dead arm + factually wrong comment
- [x] external dependency. The off-Check `aws s3 ls`/`sync` acceptance (which needs a marker

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
- Iteration delta (if iterating): Mainline accepted — do NOT change the approach or regress the 23-test suite. One targeted defect to fix (adversary finding, §6 item 4): `bucket_scoped_path` (crates/gateway-s3/src/lib.rs:427-436) uses `trim_start_matches('/')`, which strips ALL leading slashes, so the `Some(_) => None` "empty bucket segment (`//…`) → neither" arm is unreachable and its comment is false; a signed `GET //bucket?list-type=2` answers 200 instead of an error. Fix is a one-liner: use a single `strip_prefix('/')` (making `//bucket` reject as intended) or delete the dead arm and correct the comment — and add a wire test locking the chosen behaviour. Already resolved at this sign-off (do not re-litigate): the off-Check aws-cli/boto3 acceptance and its marker external-dependency are explicitly deferred until at least #511 lands bucket-marker writes (§6 items 3 and 5 ticked). The C4 cargo-deny independent-reproduction and T4 closed-PR-overlap items were not grounds for iteration and remain for the next sign-off.
- By / date: Eduard Ralph / 2026-07-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
