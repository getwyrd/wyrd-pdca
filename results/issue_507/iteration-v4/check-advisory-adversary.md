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
