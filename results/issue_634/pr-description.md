## Summary
**User impact:** Once a directory-like namespace in the metadata store grows past
about a million entries, the maintenance pass that reclaims disk space can fail
outright and stop doing *any* cleanup work — not just for the large namespace, but
for the whole pass (scrub, reconstruction and rebalancing all stop too). This can
happen after a single very large multi-part upload is retired, so it is not an
exotic edge case.

This change adds a paged way to walk such a namespace in bounded chunks instead of
requiring the whole thing to be read in one shot, so growth past today's limit no
longer breaks maintenance.

## What to look at
The new capability is a `scan_page` method that reads a namespace a bounded chunk
("page") at a time, picking up exactly where the previous page left off, in a
fixed and predictable key order. It's implemented natively for every storage
backend (the embedded store, FoundationDB, and TiKV), plus the two in-memory
simulator stores used in tests — none of them fake it by wrapping the existing
whole-namespace read.

A good way to exercise it: seed a store with more keys than one page should hold,
then walk it page by page and confirm every key comes back exactly once, in
order, even when a page boundary lands in the middle of a run of similar keys.
The new test files do exactly this (see Verification below); no application code
switches to the new method yet — that lands in follow-on changes.

## Root cause
The existing enumeration primitive reads a whole namespace or fails outright once
it exceeds a fixed cap, and two populations legitimately exceed that cap (a
deliberately unbounded retirement queue, and a large single object's per-part
cleanup ledger). Each backend's internal "is this page full" check was also
re-derived independently, so a chunked or region-sharded read could hand back a
short page that a caller would wrongly treat as "namespace fully read."

## Fix
Adds a required, bounded, cursor-based paging method to the storage trait, backed
by one shared "is this page full" rule used by every backend's read loop and by
the cursor logic itself, so the two can never disagree. Four properties are
enforced identically everywhere: results are ordered by raw byte value, a page
always starts strictly after the given cursor, a page never claims completion
early, and any key present for the whole walk is never silently skipped. Nothing
adopts the new method in this change — it only adds the primitive.

## Verification
- **Claim:** every backend can walk a namespace past today's fixed cap without
  the whole-or-fail behavior, in a fixed byte order, with the returned cursor
  never repeating or skipping a key that was present the whole time.
  **Checked:** the four properties are asserted as shared conformance clauses
  reused across every backend — `crates/metadata-conformance/src/lib.rs:428-441`
  (the clauses wired into `run_all`), exercised against the embedded backend
  (`crates/metadata-redb/src/lib.rs:73-90`, `:110-140`), FoundationDB
  (`crates/metadata-fdb/src/lib.rs:1385-1420`), TiKV
  (`crates/metadata-tikv/src/lib.rs:443-476`), and both in-memory simulator
  stores (`crates/dst/tests/support/mod.rs:291`, `:739`).
- **Checked:** the shared page-fullness rule is the single source of truth for
  both the fill loop and the cursor on every backend — `crates/traits/src/lib.rs`
  (added `page_is_full`, called from each backend's fill loop, e.g.
  `crates/metadata-fdb/src/lib.rs:1467`, `crates/metadata-tikv/src/lib.rs:550`).
- **Test:** `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs` and
  `crates/metadata-redb/tests/scan_page.rs` — both new files; on `main` they do
  not compile (`scan_page` does not exist yet), and pass in full once this change
  is applied. The suite also includes deliberately-wrong store doubles (wrong key
  order, inclusive cursor, early "no more" answer, dropped stable key) to confirm
  each clause actually catches the defect it targets, not just passes vacuously.
- **Test:** a real-backend escape check — `crates/metadata-redb/tests/scan_page.rs`
  seeds a store past a lowered cap and asserts the old whole-namespace read fails
  with the existing cap error while the new paged walk still returns every key.

Fixes #634
