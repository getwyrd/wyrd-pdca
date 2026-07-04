# Native paged prefix scan for the TiKV metadata backend

## Summary
Listing a directory (any key prefix) through the TiKV metadata backend could
silently return an **incomplete** set of entries whenever the prefix held more
than one network page's worth of keys — and because garbage collection builds its
never-reclaim safety set from a full `inode:` listing, a truncated listing is
silent data loss, not just a slow response. This change replaces the single
unbounded read with an internally paged scan that returns the **complete** set
observed at one consistent point in time, or **fails loud** rather than ever hand
back a truncated result.

## What to look at
- `crates/metadata-tikv/src/lib.rs` — `scan` (around `lib.rs:487`) is the rewrite:
  one transaction is opened before the loop and held across every page
  (`lib.rs:496`), so the whole result is read at a single timestamp.
- The paging and cap **decisions** are factored into a dependency-free `paging`
  module (`lib.rs:126`) — `next_page_start` (`lib.rs:182`), `after_page`
  (`lib.rs:210`), and the typed `ScanCapExceeded` error (`lib.rs:155`) — so they
  compile and unit-test on any machine, with or without TiKV.
- To exercise it: `cargo test -p wyrd-metadata-tikv --lib` runs the paging unit
  tests everywhere; the full at-scale listing runs against a throwaway single-node
  TiKV via `cargo xtask tikv-conformance` (it skips cleanly when no TiKV endpoint
  is configured, so the default `cargo xtask ci` stays green with no cluster).

## Root cause
The prior skeleton `scan` issued one `txn.scan(range, u32::MAX)` — a single
unbounded range read that pulled an entire prefix over the network in one shot,
with no internal paging, no completeness guard, and undocumented read-snapshot
semantics. A prefix larger than one page was therefore never guaranteed to come
back whole or as a single consistent view.

## Fix
- **Paged under one snapshot.** `scan` opens one `begin_pessimistic` transaction
  — a single fixed read timestamp — and pages `[prefix, prefix_upper)` in
  `PAGE_SIZE` chunks inside that one transaction, advancing the cursor strictly
  past each full page's last key until a short page ends the range. The whole
  materialized set is one consistent cut; listing order stays unspecified (every
  caller collects into a set/map).
- **Complete-or-fail-loud.** `after_page` checks an interim per-listing cap first;
  on breach `scan` rolls back and returns `Err(ScanCapExceeded)` with **no**
  partial `Vec` (`lib.rs:521-530`). The error names the overflowing prefix and
  states it refused to truncate; the caller that owns telemetry (GC/custodian)
  surfaces the operator-visible signal, so the backend gains no new dependency.
- **Documented contract.** A module-level doc (`lib.rs:301`) records the guarantee:
  a fresh snapshot per `get`/`scan`, one consistent cut across all pages of a
  `scan`, and why `rename`'s read-then-commit stays safe through the commit-time
  precondition re-check under the locking rule (`get_for_update`) rather than read
  freshness. `commit` and the metadata trait are untouched.

## Verification
- **Claim:** a prefix spanning more than one internal page returns the complete
  `(key, value)` set as a single consistent cut — never a truncated subset.
  - **Checked:** `crates/metadata-tikv/src/lib.rs:487-536` — one transaction held
    across every page; cursor advance via `paging::after_page` (`lib.rs:210`) /
    `next_page_start` (`lib.rs:182`) never re-yields nor skips a key.
  - **Test:** `crates/metadata-tikv/tests/scan.rs:394` inserts more than one page
    of entries under a prefix (plus a neighbouring-prefix decoy) and asserts the
    complete set with correct values and no decoy. Endpoint-gated: skips without a
    TiKV endpoint, runs for real under `cargo xtask tikv-conformance`.
- **Claim:** an over-cap listing fails loud with an error and never a partial `Vec`.
  - **Checked:** `crates/metadata-tikv/src/lib.rs:521-530` — cap breach rolls back
    and returns `Err(ScanCapExceeded)`; the cap is checked before short-page
    termination so an over-cap set cannot slip through as a "complete" short page.
  - **Test:** the `paging` unit tests in `crates/metadata-tikv/src/lib.rs`
    (`a_breach_of_the_cap_fails_loud_never_truncates`,
    `cap_is_checked_before_termination`, `scan_cap_exceeded_error_is_operator_visible`)
    fail pre-fix and pass post-fix. Temporarily disabling the cap check made exactly
    those two tests fail (2 of 10), confirming they rest on the guard, not on its
    absence.
- **Claim:** the read-consistency guarantee is documented, not merely implied.
  - **Checked:** module contract at `crates/metadata-tikv/src/lib.rs:301` and the
    `get` doc at `lib.rs:462`.
- **Whole gate:** `cargo xtask ci` passes with no TiKV present (fmt, clippy
  `-D warnings`, build, whole-tree tests, deny, conformance); the at-scale
  `tests/scan.rs` skips cleanly, and `xtask/src/main.rs:502` wires `scan` into the
  endpoint-gated `tikv-conformance` job.

The live at-scale run (complete-set + consistent-cut + forced cap breach) executes
against a throwaway single-node TiKV rather than in the default CI, so a maintainer
should confirm that run before merge. The consistent-cut guarantee under concurrent
mutation is pinned by a companion shared-conformance change (#419); this PR ships
the at-scale completeness and fail-loud-cap proof that #419 does not itself cover.

[Fixes #254](https://github.com/getwyrd/wyrd/issues/254)
