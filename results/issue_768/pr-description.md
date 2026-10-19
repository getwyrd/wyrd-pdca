## Summary
**User impact:** When a cluster runs several gateways and one of their clocks is a few seconds
fast, overwriting a file can make its "last modified" time go *backwards*. Sync tools such as
`aws s3 sync` decide what to copy by comparing size and time, so they then see the new version
as older and skip it when the size hasn't changed. The mirror keeps the old content and nothing
reports an error. The same fast clock also makes S3 report a modification time in the future,
which HTTP forbids.

This PR makes a file's stored modification time always move forward across overwrites, and
makes the S3 API never show a time later than the response's own clock.

## What to look at
- **The rule** is one small function in core: each overwrite records "the publisher's clock, or
  one millisecond past the time it replaces, whichever is later". It never moves a stored time
  backwards, even one left by a badly wrong clock. Its doc comment states the whole contract.
- **The one new refusal:** an overwrite that carries no time over a file that has one is
  refused and writes nothing. Both S3 PUT paths always send a time, so no real write is
  affected; it only closes the one way the order could reset.
- **What S3 shows:** a small helper picks the earlier of the stored time and the gateway's
  clock. It is used for the `Last-Modified` header, the listing, and the date-based
  conditional requests, and the response's `Date` comes from that same clock reading.
- **To try it:** `cargo test -p wyrd-server --test last_modified_monotonic` (9 tests, about
  0.1 s). They seed a file stamped an hour or two days in the future, overwrite it through the
  real gateway, and read the result back from storage and over a real S3 connection.
- **Known limits** (by design, written into the architecture doc):
  - S3 still reports whole seconds, so two overwrites in the same second can show the same
    `Last-Modified`. Clients that must see every change should compare `ETag`.
  - A mirror that *already* copied a future time (from a gateway without this fix) will see
    later overwrites as older until real time catches up. Re-sync it by `ETag` or with a full
    copy. The same applies in the upload direction: a local tool that uploads only "newer"
    files won't push a same-size edit of such a file. Fixing a stored future time needs an
    explicit operator action, which this PR does not add.
  - Keeping the window between gateways at milliseconds relies on host clock sync (#902),
    which ships in the same release.
- **Size:** about 140 KB of diff. Most of it is tests and doc comments; the production logic
  is the core rule plus threading one clock reading through the S3 response builders.

## Root cause
Each gateway stamps `modified` from its own clock on both PUT paths
(`crates/server/src/lib.rs:277` buffered, `:452` streaming), and the core overwrite commits
stored that stamp as given, never comparing it with the one it replaced. The S3 gateway then
rendered the stored time as-is and evaluated `If-Modified-Since` / `If-Unmodified-Since`
against it, so a fast clock's stamp showed up as a future `Last-Modified`.

## Fix
- **Core (`crates/core/src/metadata.rs`):** new `overwrite_modified` (`:2373-2402`) returns
  `max(published, prior + 1)` (saturating at `u64::MAX`, no wrap), passes a legacy unstamped
  prior through, and refuses an unstamped overwrite of a stamped prior with the new typed error
  `UnstampedOverwrite` (`:2299-2318`). It logs a warning when the prior is more than a day
  ahead (`MODIFIED_LEAD_WARN_MILLIS`, `:2287`) but still orders after it. Both public overwrite
  commits call it before writing anything: `commit_chunk_map_superseding` (`:2250`) and
  `commit_chunk_map_superseding_leased` (`:2451`, the one the gateway uses). The refusal is a
  typed error, not `CommitOutcome::Conflict`, because callers retry a conflict and a retry
  would not add a time. Repair commits still preserve `modified` unchanged.
- **S3 gateway (`crates/gateway-s3/src/lib.rs`):** `shown_modified` (`:3084-3086`) is
  `min(stored, now)`, and `date_header` (`:3099-3103`) renders `Date` from the same reading.
  It is applied to the ranged GET (`:2452`), full GET (`:2489`), HEAD (`:2534`, before the
  precondition check), the `304` renderer (`not_modified_response`, `:2731`), and each listing
  row (`render_contents`, `:1059`). A read never rewrites the stored time.
- **Docs:** the rule, what S3 shows, the known limits and how this stays within ADR-0047's
  meaning of `modified` are added to the object-metadata section of
  `docs/design/architecture/05-building-block-view.md` (`:205-240`), and to the doc comments
  on the `modified` fields and `WritePlan` (`crates/core/src/write.rs`). No ADR is changed.
- **Wire changes to expect:** every response that shows a `Last-Modified` now carries a
  handler-set `Date`; a stored time past year 9999 is shown as the response time instead of
  being omitted; on a host whose clock reads before 1970 nothing is capped and `Date` is left
  to hyper.

## Verification
- **Claim:** an overwrite stores exactly `prior + 1 ms` when the prior is ahead of the clock,
  on both PUT paths, including a prior two days ahead (no cap, no automatic correction).
  - **Checked:** `crates/core/src/metadata.rs:2373-2402` (the rule), called at `:2250` and
    `:2451`.
  - **Test:** `crates/server/tests/last_modified_monotonic.rs:286`, `:293`, `:301`. They seed
    a mid-second time, so a whole-second or `+1 s` step would also fail.
- **Claim:** the rule's exact values hold on both core commits, including saturation at
  `u64::MAX`, repeated overwrites, and the refusal (typed error, record byte-for-byte
  unchanged).
  - **Checked:** `crates/core/src/metadata.rs:2299-2318`, `:2373-2402`.
  - **Test:** `crates/server/tests/last_modified_monotonic.rs:488`, plus core unit tests at
    `crates/core/src/metadata.rs:5633`, `:5638`, `:5853` (the more-than-a-day warning).
- **Claim:** the ordinary cases are unchanged. A create, or an overwrite of a past time,
  records the current clock, and one healthy gateway shows no lead over the clock.
  - **Test:** `crates/server/tests/last_modified_monotonic.rs:310`, `:327`, `:1181`. These
    pass on `main` too, by design.
- **Claim:** over a real S3 connection, a future stored time is shown as the response time and
  a past one as stored, for HEAD, GET, ranged GET, the `304`, the listing, and both date
  conditionals. No response shows a time later than its own `Date`.
  - **Checked:** `crates/gateway-s3/src/lib.rs:1059`, `:2452`, `:2489`, `:2534`, `:2731`,
    `:3084-3103`.
  - **Test:** `crates/server/tests/last_modified_monotonic.rs:1142` (signed raw HTTP/1.1
    against a real `S3Gateway` on loopback), plus router-level tests at
    `crates/gateway-s3/src/lib.rs:5369`, `:5414`, `:5447`.
- **Red → green:** with the production change reverted and the test kept, 5 of the 9 tests
  fail by assertion (the stepping legs, the core values and the wire leg). The other 4 are the
  ordinary-case checks above, which pass on `main` by design. All 9 pass with the change.
- **Gates:** `cargo xtask ci` passes on the branch. On the changed files, `cargo mutants
  --in-diff` reports 35 mutants caught, 4 unviable and none missed, and diff coverage is 99.5%
  (628 of 631 changed lines executed).
- **Not covered:** real skew between two hosts can't be reproduced in one process. A stored
  future stamp is a faithful stand-in, because the store doesn't record which gateway wrote it.

Fixes #768
