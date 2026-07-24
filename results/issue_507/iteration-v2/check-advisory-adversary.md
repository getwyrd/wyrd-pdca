# check-advisory-adversary.md — issue 507 / list-objects-v2 (iteration 2)

Adversarial pass. The red→green evidence was re-run **independently** (scratch clone at
wave base `07d0244` + the ADDED test only): all 14 tests fail by **assertion** (400
`InvalidRequest` off the wire, compile succeeds — no new production symbol imported), and
all 14 pass on the patched tree. The proof drives the production path end-to-end via a
stock `aws-sdk-s3` client over a real loopback listener; it is not a parallel
re-implementation, and the ETag oracle is an independent SHA-256. The C4-verify PASS in
`check-gates.json` is corroborated, not merely trusted. All six iteration-1
carry-forwards are genuinely addressed (max-keys=0, decoded denylist, delimiter+max-keys
chaining under truncation, start-after/encoding-type, tempered seam-cost doc,
list-type≠2 → 400 / NextMarker gated on delimiter). Three refutations still landed:

- NEEDS-HUMAN [impl] — **Denylist gap: `GET /bucket?versioning` is silently answered with a
  listing document** — the exact failure class the brief's routing decision 1 forbids.
  `UNSUPPORTED_SUBRESOURCES` (`crates/gateway-s3/src/lib.rs:330-361`) lists `versions` but
  not `versioning`, so the bucket route's fence (`lib.rs:1247`) passes it through to
  `list_objects`. Empirically demonstrated with a scratch probe: stock-SDK
  `get_bucket_versioning()` against the patched tree receives `<ListBucketResult>` and dies
  with `XmlDecodeError: expected VersioningConfiguration but got ListBucketResult` instead
  of a clean 501 — and `aws s3 sync`-adjacent tooling (rclone, boto-based scripts) does
  probe versioning. Same gap for `intelligent-tiering`, `ownershipControls`,
  `policyStatus`, `metadataTable`. Fix is a few list entries; add a probe test.
- NEEDS-HUMAN [impl] — **The gating C4-ci red is caused by this patch, not the
  environment**: `typos` flags `mis-decoding` in the new comment at
  `crates/gateway-s3/src/lib.rs:556` (verified by running `typos` on the target: the sole
  finding, `556:81 mis → miss/mist`). One-word reword (or a `typos.toml` exception)
  un-reds the only failed deterministic gate; nothing deeper hides behind it.
- NEEDS-HUMAN [impl] — **v1 `NextMarker` emits the wrong value and the marker resume can
  double-emit a group across implementations.** `render_list_v1` emits `page.next_key` —
  the last *consumed* raw key (`lib.rs:770,800`, e.g. `a/1`) — where AWS's `NextMarker` is
  the last item *returned* (the common prefix `a/`). Correspondingly, the resume filter
  `o.key > marker` (`lib.rs:491`) treats a marker naming a common prefix as an ordinary
  key: concrete case — keys `{a/1,a/2,b/1}`, `GET /bucket?delimiter=/&marker=a/` re-emits
  `CommonPrefixes a/` where AWS (whose own NextMarker would be `a/`, and which cannot
  infinite-loop on it) skips the whole group. Self-chaining with *our* markers works
  (proven by the tests), but a client resuming from a stored last-CommonPrefix — the
  documented AWS v1 pattern — receives duplicates. The test
  `crates/server/tests/s3_list_objects.rs:414-438` asserts only `next_marker().is_some()`,
  never its value, so it locks the deviation in unnoticed.

Attempted and could NOT refute:
- Pagination correctness under truncation with delimiter (group-consume/resume,
  `lib.rs:465-535`): probed max-keys=1 chaining, group-at-budget-boundary, resume exactly
  at end-of-list — token = last *consumed* key makes mid-group resume unreachable for a
  conforming v2 client; no double-emit found (test at `s3_list_objects.rs:466` is real).
- Token robustness: empty `continuation-token=` → `base64_decode` rejects empty input
  (`crates/gateway-s3/src/checksum.rs:180`) → 400, not a silent restart; non-canonical
  base64 rejected; percent-encoded `+`/`=` in tokens decode correctly.
- Percent-encoded subresource dodge on the bucket route (`?%61cl`, `?upload%73`) — closed
  by `unsupported_subresource_decoded` (`lib.rs:380`).
- Bucket/`foobar` prefix spill — fenced by the trailing `/` in the scan prefix
  (`crates/server/src/lib.rs:473`); pending/torn inodes correctly excluded (`:492-497`).
- Note (pre-existing, not this diff's defect): a percent-encoded `/` in the *bucket*
  segment (`GET /bu%2Fcket`) decodes to `bu/cket` and aliases into bucket `bu`'s dirent
  namespace; listing merely makes the existing flat-encoding aliasing observable. The
  bucket-record read fences the listing itself (404). Belongs with #511 name validation.
