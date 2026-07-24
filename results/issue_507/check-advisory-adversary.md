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
