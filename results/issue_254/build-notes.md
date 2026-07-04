# Build notes — issue 254 / native-prefix-scan-read-consistency (M4.3)

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (worktree
`$PDCA_WORKTREE`, base `ac203b1` — the M4.2 merge). Diff confined to one crate plus
the xtask job wiring, as the brief scopes it.

## What the Success criterion demands, and where each half lands

1. **Completeness-or-fail-loud** (BINDING; #262 + ADR-0011). A `scan(prefix)` returns
   the complete matching set, or `Err` — never a silently truncated `Vec`.
2. **One consistent snapshot across all internal pages** (BINDING; #261 + ADR-0015).
3. **A documented read-consistency contract** (BINDING).
4. Shared conformance still passes; no order-dependence regresses; `cargo xtask ci`
   green with no TiKV.

## The change (cite `path:line` on the target, post-patch line numbers)

- **Pure paging/cap decision logic — `crates/metadata-tikv/src/lib.rs:126` `pub mod
  paging`** (dependency-free, exactly like the existing `keyspace` module `lib.rs:31`,
  so it compiles and unit-tests on every machine — no `tikv-client`, no runtime):
  - `PAGE_SIZE = 1024` (`lib.rs:134`) — keys per internal range read.
  - `SCAN_CAP = 1<<20` (`lib.rs:145`) — interim ceiling on total materialized results.
  - `next_page_start` (`lib.rs:182`) — cursor advance: `last_key || 0x00`, the smallest
    key strictly after the last returned, so paging never re-yields nor skips a key.
  - `after_page` (`lib.rs:210`) — the load-bearing decision: cap checked **first**
    (`total > cap → CapExceeded`, `lib.rs:217-219`), then short-page → `Done`, full page
    → `Continue(next_page_start(last))`.
  - `ScanCapExceeded` (`lib.rs:155`) — a descriptive typed `Error` (Display names the
    prefix + cap + "truncat…") used as the store's fail-loud `Err`.
- **Internally-paged native scan — `crates/metadata-tikv/src/lib.rs:487` `async fn
  scan`** — replaces the M4.1 single `txn.scan(range, u32::MAX)` shortcut. **One**
  `begin_pessimistic` txn (one start TSO) is opened before the loop and held across
  every page (`lib.rs:496`), pages `[cursor, prefix_upper)` with `PAGE_SIZE`, advances
  the cursor via `after_page`, accumulates the owned `Vec` (order unspecified), and on
  `PageStep::CapExceeded` rolls back and returns `Err(ScanCapExceeded)` with **no**
  partial `Vec` (`lib.rs:521-530`). The pre-existing `rollback_then` drop-safety
  discipline is preserved on the scan-error path.
- **Read-consistency contract doc — `crates/metadata-tikv/src/lib.rs:301`** — a
  module-level `//!` block on `mod store` recording the #261 decision: fresh-TSO
  snapshot per `get`/`scan`; one consistent cut across all pages of a `scan`;
  completeness-or-fail-loud; and *why* `rename`'s read-then-commit is safe — the commit
  precondition re-check under the locking rule (`get_for_update`, unchanged
  `commit` at `lib.rs:522`-region), **not** read freshness. `get` (`lib.rs:462`) gets a
  doc noting its fresh-TSO snapshot; its code already opened one pessimistic txn per
  call, so it was already aligned — the slice documents it, no behaviour change.
- **xtask wiring — `xtask/src/main.rs:194` `run_tikv_conformance_test`** — adds `"scan"`
  to the endpoint-gated test list so `cargo xtask tikv-conformance` runs the at-scale
  proof against the throwaway `deploy/` TiKV alongside `conformance`/`contention`.

## Design decisions / rationale

- **Kept `begin_pessimistic`, held across pages** — the brief's verified backend fact
  (brief §"Verified backend facts") says a pessimistic txn already reads at one start
  timestamp, so holding the one txn across all pages *is* the #261 consistent cut. I did
  **not** switch to a lighter snapshot API: the tikv-client 0.4 shape of
  `client.snapshot(ts,..)`/`begin_optimistic` is UNVERIFIED at Plan (#260 caveat) and a
  switch buys nothing for correctness. Simplest correct implementation.
- **Fail-loud form = descriptive typed `Err`, audit signal caller-side (option (a) of
  the brief's NEEDS-HUMAN).** `metadata-tikv` has no telemetry/tracing dep today; pushing
  an emit into the store is a new-dep ADR-0003/ADR-0016 review. The GC/custodian caller
  (`custodian/src/gc.rs:222`, the `scan(b"inode:")` #262 protects) already owns telemetry
  and surfaces the ADR-0011 operator-visible signal. `ScanCapExceeded` is downcastable so
  the caller can distinguish "too big, fail loud" from a backend fault. Rejected option
  (b) (store-side tracing emit) because it costs a new dependency for no correctness gain.
- **Cap value = 2^20.** Far past any legitimate single directory, bounds gateway heap.
  Memory overshoot before the breach is detected is ≤ `PAGE_SIZE` (one page), because the
  cap is checked after each page; `after_page` documents this. Flagged as possibly
  product-facing (brief NEEDS-HUMAN) — it lives in the documented contract, not a bare
  constant.
- **Cap semantics** — `total > cap` (strict), so a set of *exactly* `cap` is allowed;
  breach detection survives page-boundary alignment (unit-tested at `lib.rs:263`,
  `lib.rs:277`).
- **Order stays unspecified** — every scan caller is order-independent (brief §Verified
  backend facts audit); the at-scale test asserts set-membership, not order.

## Red→green (Check-observable) + demonstrated load-bearing red

The named `tests/scan.rs` is endpoint-gated and **skips** without a TiKV (verification
posture DEFERRED, as the prior M4 slices) — its red→green is not observable at Check; it
runs for real under `cargo xtask tikv-conformance`. The **Check-observable** red→green is
the pure `paging` unit tests in `src/lib.rs` (run by `cargo xtask ci`'s
`cargo test --workspace`).

Demonstrated the cap logic is load-bearing per the #146 forcing function: temporarily
negated `after_page`'s cap check (`if total > cap && false`) and re-ran —
`a_breach_of_the_cap_fails_loud_never_truncates` and `cap_is_checked_before_termination`
**FAILED** (2 failed, 8 passed); restoring `if total > cap` → all 10 pass. So the tests
rest on the guard, not on non-existence.

## Verification performed

- `cargo test -p wyrd-metadata-tikv --lib` — 10 passed (keyspace + paging units).
- Negation run — 2 targeted FAILED, proving the cap tests load-bearing; restored.
- `cargo check/clippy -p wyrd-metadata-tikv --features tikv --tests` — clean (tikv-client
  tree was cached; the store rewrite + at-scale test both typecheck and lint under the
  real feature, so the deferred path is not merely unbuilt).
- `cargo fmt -p wyrd-metadata-tikv -- --check` — clean (commit-hook ready).
- `./engine/xtask.sh ci` (the authoritative gate, in `$PDCA_WORKTREE`, no TiKV) —
  **"xtask ci: all checks passed"** (fmt, clippy `-D warnings`, build, whole-tree test,
  cargo-machete, cargo-deny, conformance, DST). `tests/scan.rs` skipped cleanly.

**Deferred (endpoint-gated) at-scale proof — NOT run here** (no TiKV in the worktree): the
live `cargo xtask tikv-conformance` run against `deploy/` single-node TiKV that exercises
`tests/scan.rs` (set size = `PAGE_SIZE*3/2 + 7 = 1543`, spanning >1 page; asserts complete
set + consistent cut + neighbouring-prefix decoy excluded) and a forced cap breach. This is
a pre-declared C2/C4 sign-off item (brief §Verification posture / §Known NEEDS-HUMAN), for
the human to run/observe at sign-off.

## Scope item (d) NOT done — and why (surface at sign-off)

Brief scope (d) — "wire #419's three read-consistency properties (read-after-commit,
rename-race, scan-consistent-cut) against `TikvMetadataStore`" — **presupposes #419 has
landed** those shared property fns into `wyrd-metadata-conformance`. It has **not**:
`crates/metadata-conformance/src/lib.rs` today exposes only `contract_commit_and_get`,
`contract_scan_by_prefix`, `contract_require_absent_gates`, `contract_require_value_gates`
— no `*_consistent_cut` / `*_read_after_commit` / `*_rename_race`. Authoring those
properties is **explicitly out of scope** for #254 (brief §Scope "Out of scope"; they are
#419's, landing vs redb first). So I did not wire (d), and did not fabricate the
properties. This matches the brief's `Depends on: 419` and the "human override" note: if
the maintainer ships #254 ahead of #419, (d) is deferred until #419 lands and TiKV
inherits the shared executable pin then. #254's own paging-at-scale completeness +
fail-loud cap (its part that #419 does *not* cover) are fully implemented and tested here.

## Known NEEDS-HUMAN carried (from the brief, not blockers)

- Endpoint-gated red→green for `tests/scan.rs` (deferred to `tikv-conformance`).
- Read-consistency doc = a documented already-accepted decision (#261/ADR-0015 Option C);
  mints no new ADR.
- ADR-0011 audit-signal concrete form: chose option (a) (descriptive `Err`, caller emits).
- Interim cap value 2^20 may be product-facing.
- Sequencing vs #419 (scope (d) deferred, above).
