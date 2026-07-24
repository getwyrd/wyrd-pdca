# build-notes.md — issue 507 / list-objects-v2 (iteration 5)

## Scope of this iteration

The iteration-4 sign-off accepted the mainline ("keep the approach — the mainline is sound")
and pulled two scope-level defects into the contract. Per the brief's *Iteration-5 basis*, I
re-applied `iteration-v4/patch.diff` to the target worktree unchanged as the starting point,
then made ONLY the two deltas plus the minor, and their tests. Nothing else in the v4 design
changed (all 15 accumulated iteration-1..4 fixes are locked and stayed green).

Base: `getwyrd/wyrd @ main`, worktree `$PDCA_WORKTREE` at `07d0244` (wave 0, brief base). The
patch is regenerated with `git diff` over the full change set (v4 + the deltas). Target citations
below are `path:line` on that worktree after the patch is applied.

## Delta 1 — implement `encoding-type=url` (scope change; replaces the v4 501 rejection)

**Defect (sign-off):** v4 answered `501 NotImplemented` to any `encoding-type`, with a
factually-wrong comment ("Stock aws-cli ls/sync do not send it"). botocore injects
`encoding-type=url` into EVERY ListObjects/V2 request (verified in the brief), so the 501 refuses
the exact stock aws-cli / boto3 / rclone clients the Goal names.

**Fix (all in `crates/gateway-s3/src/lib.rs`):**
- `list_objects` parses `encoding-type` into an `encode: bool`: `url`→`true`, absent→`false`,
  any other value→`400 InvalidArgument` (AWS behaviour). The factually-wrong comment is deleted.
- New `url_encode_value` (`lib.rs:~843`) percent-encodes the UTF-8 bytes leaving only the
  unreserved set `A-Za-z0-9-_.~` plus `/` literal; space→`%20` (never `+`). `project_value`
  (`lib.rs:~861`) applies it THEN xml-escapes (ADR-0046 XML decision: escape stays the outer
  invariant), or xml-escapes only when `encode==false`.
- `render_list_v2` / `render_list_v1` project `Key`, `Prefix`, `Delimiter`,
  `CommonPrefixes`→`Prefix`, and the resume echoes `StartAfter` (v2) / `Marker`+`NextMarker`
  (v1); emit `<EncodingType>url</EncodingType>` when `encode`. The opaque
  `ContinuationToken`/`NextContinuationToken` are xml-escaped ONLY — never URL-encoded — so a
  returned token resumes verbatim. Encoding is RENDER-TIME ONLY: `compute_page` filtering,
  grouping, resume comparison and the token payload all operate on raw keys (unchanged).

Encoding is proven against the exact botocore oracle from the brief: key `a&b/c d` →
`<Key>a%26b/c%20d</Key>`.

The v4 test `list_v2_encoding_type_is_rejected_not_silently_ignored` is removed (obsolete). It is
replaced by **raw signed-HTTP** wire tests (the SDK paginator does not inject `encoding-type` by
default — the blind spot that let four green C4 rounds miss this; raw HTTP asserts the exact wire
bytes with no SDK decode layer). SigV4 trap handled: the peer harness helper signs an empty query
(`s3_object_metadata.rs:81-83`); every new wire test carries a query, so `signed_headers_q` passes
the query to `sign` (which canonicalizes it), or the green leg would 403.

## Delta 2 — restrict the `r == cp` group-collapse to the v1 marker resume

**Defect (sign-off, reproduced on the wire):** v4's `compute_page` predicate
`resume_after.is_some_and(|r| r == cp.as_str() || r >= last_raw)` collapsed a whole delimiter
group whenever the resume value equalled the common prefix — but that `== cp` equality is only
correct for the server-issued v1 `NextMarker` (which names the CP by design). A CLIENT-chosen v2
`start-after` exactly equal to a common prefix (`start-after=a/`, the folder-marker workflow) was
wrongly collapsed: bucket `{a/1,a/2,b}` with `?list-type=2&delimiter=/&start-after=a/` returned
`CommonPrefixes=[]` instead of `[a/]`.

**Fix:** a `ResumeKind` discriminator (`V2` / `V1Marker`) threads into `compute_page`; the `== cp`
clause is applied ONLY on `V1Marker`. `group_consumed` is now
`r >= last_raw || (resume_kind == V1Marker && r == cp)`. No v2 behaviour is lost: a server-issued
v2 token always encodes the group's last raw key, so it collapses via `r >= last_raw` alone. v1
`marker` keeps `== cp` (the documented AWS resume). Precedence when both v2 resume params arrive
is unchanged from v4 (the token wins, `start-after` consulted only when no token is present) and is
now exercised by an explicit test (`list_v2_continuation_token_wins_over_start_after`).

## Minor (SHOULD) — `<StartAfter>` echo

`render_list_v2` now emits `<StartAfter>` when the request carried `start-after` (URL-encoded
under `encoding-type=url`). Carried via `request_start_after` in `list_objects`.

## clippy refactor (commit-readiness, not behaviour)

Adding `start_after`+`encode` pushed `render_list_v2` to 8 args (clippy `too_many_arguments`,
7/7 cap). I bundled the request-echo fields (`bucket`/`prefix`/`delimiter`/`max_keys`/`encode`)
into a `ListView<'_>` struct shared by both renderers — render_list_v2 → 4 args, render_list_v1 →
3. Pure refactor; the 66 gateway-s3 unit tests + 23 integration tests stay green. The test's
`dechunk` was rewritten `loop{match}` → `while let` (clippy `while_let_loop`).

## `fetch-owner` status quo (brief open question)

Unchanged from v4: `fetch-owner` is omitted and the stock SDK paginator suite passes without it.
No `<Owner>` is emitted; not required by the paginator.

## Refute-your-own-test (forced, recorded)

- **(a) Genuine red?** Yes, two legs, both via the project runner
  (`engine/scripts/run-verify.sh`):
  - *Whole-feature red (C4-verify):* production reverted, all 23 added tests kept → **23/23 fail
    by assertion** on the wave base (`400 InvalidRequest`, `split_bucket_key` returns `None`) —
    no compile-error red (the test imports no new production symbol). Gate verdict: "PASS — red
    without the fix, green with it."
  - *Fix-specific red (this iteration's two deltas):*
    - Delta 2: reverting only the restriction (apply `== cp` on both paths) →
      `list_v2_start_after_equal_to_common_prefix_still_returns_rollup` **FAILS**
      (`left: [], right: ["a/"]`) while the two v1 marker-resume tests stay GREEN — so the new
      test binds *this* fix and no v1 behaviour is lost.
    - Delta 1: forcing `encode=false` on the `url` arm → the 4 `encoding_type_url` tests **FAIL**
      (the exact encoded elements are absent) while the `encoding-type=broken`→400 test stays
      green (it tests validation, not rendering). So the encoding tests bind the render fix.
    Both files restored byte-identical (backup diff) and re-run green afterwards.
- **(b) Production path?** Yes. Every assertion drives the shipping HTTP surface — the SDK client
  (`list_objects_v2()`/`list_objects()` paginator) or raw signed HTTP over a real loopback
  listener — against the production `S3Gateway` + `Gateway` + `RedbMetadataStore` stack. No
  production symbol is re-implemented in the test; the ETag oracle is an independent SHA-256, and
  the encoding assertions read the exact wire bytes with no SDK decode in between.
- **(c) Fixture includes the fault?** Yes. Delta 2's test seeds `{a/1,a/2,b}` and sends
  `start-after=a/` (equal to the common prefix) — the fixture contains the group the old code
  collapsed. Delta 1's tests PUT keys with `&`/space (`a&b/c d`, `dir a/leaf z`, `g h/1`) so the
  encoded form differs from the raw form, and the opaque-token test uses a 5-byte resume key
  (`p2 xy`) whose base64 carries `=` padding — a char url-encoding *would* change to `%3D` — so
  "no `%` in the token" is a real opacity check, not vacuous.

## Verification run (project runner)

- `engine/scripts/run-verify.sh` (C4-verify, on the final `patch.diff`) → **PASS — red without
  the fix (23/23), green with it (23/23)**.
- `cargo test -p wyrd-server --test s3_list_objects` → 23 passed.
- `cargo test -p wyrd-gateway-s3 --lib` → 66 passed (incl. the base64 round-trip vector).
- `cargo fmt` (touched crates) → clean; `cargo clippy -p wyrd-gateway-s3 -p wyrd-server
  -p wyrd-gateway-core -p wyrd-core --tests -- -D warnings` → clean; `typos` over the six touched
  files → clean. These are the fmt/clippy/typos/test legs the target's commit hooks and the
  `cargo xtask ci` gate mirror, so a publish commit has nothing to reject.

## No external dependency gap

Everything ran in-process on the base toolchain (redb-in-memory + fs-tempdir + loopback). No
NEEDS-HUMAN external dependency. The off-Check `aws s3 ls`/`sync` acceptance (which needs a marker
backfill or #511, and the registered `aws` CLI doctor row) is the human's Check-time manual step,
not a build blocker — and with Delta 1 a backfilled stack now works with stock aws-cli, which v4
did not.
