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
