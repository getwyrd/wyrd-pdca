# Add a uniform regression net for mixed-era placement resolution

## Summary
Wyrd has to keep reading and self-healing objects written before chunk
`placement` records existed (pre-M3 / mixed-era data): such a record carries an
empty or short `placement` vector, and every fragment whose entry is absent must
resolve to its identity D-server. That behaviour is correct today, but it was
guarded unevenly — most consumers were only tested against full-length placement
vectors, where the fallback is never reached — so a future change to any one
consumer could silently break old objects with no test failing. This change is
test-only: it adds the missing coverage so every placement consumer is pinned for
empty, short, and full placement.

## What to look at
The change touches six test files only; no production code changes.

- `crates/core/tests/placement_record.rs` — the read path for empty/short vectors.
- `crates/custodian/tests/gc.rs`, `.../scrub.rs`, `.../reconstruction.rs`,
  `.../rebalance.rs` — the maintenance consumers (GC reclamation, scrub, repair,
  drain/evacuation).
- `crates/server/tests/dst_erasure.rs` — an end-to-end simulation asserting the
  read path and the maintenance path resolve an empty-placement object the same
  way across all seeds.

The crux is that a *full* placement vector exercises none of the fallback, so a
full-only test cannot catch a fallback regression — each consumer needs explicit
empty/short cases. To exercise:

```
cargo test -p wyrd-core      --test placement_record
cargo test -p wyrd-custodian --test gc --test scrub --test reconstruction --test rebalance
cargo test -p wyrd-server    --test dst_erasure
cargo xtask ci   # the whole gate (fmt, clippy -D warnings, build, tests, deny, conformance, DST sweep)
```

## Root cause
There is a single authoritative resolver for fragment placement —
`ChunkRef::placed_dserver` (`crates/core/src/metadata.rs:119-124`) — which falls
back to D-server `i` whenever `placement[i]` is absent, and the read path, GC,
scrub, reconstruction, and rebalance all route through it. The resolver and its
consumers are already correct on `main`, but the test suite covered the
empty/short (fallback) cases for only some consumers and only at small schemes,
leaving the others free to regress to raw-vector indexing undetected.

## Fix
Add the missing matrix cells, modelled on the existing GC empty/short cases and
reusing the existing fixtures:

- **Per consumer × placement shape:** empty (no-erasure and Reed-Solomon), short,
  and full coverage for read, GC, scrub, reconstruction, and rebalance, including
  at least one Reed-Solomon {6,3} case per consumer (closing the scheme-size gap).
- **Re-placement pin:** a reconstruction case that takes a chunk committed with an
  empty `placement`, loses a D-server, and asserts the re-committed record is
  full-length (`== fragment_count()`) with the rebuilt fragment in a distinct
  domain — never a short/empty write-back.
- **End-to-end agreement:** a deterministic-simulation scenario seeded with an
  explicit empty-placement chunk, asserting the read path and the maintenance-side
  resolution agree across all seeds and at the pinned regression seed.

## Verification
- **Claim:** one authoritative resolver applies the identity-placement fallback
  for empty/short vectors.
  - **Checked:** `crates/core/src/metadata.rs:119-124` on `main` —
    `placed_dserver` returns `placement.get(i).copied().unwrap_or(i)`.
- **Claim:** every placement consumer routes through that resolver.
  - **Checked** on `main`: read — `crates/core/src/read.rs:103-105`
    (`fragment_dserver` delegates to `placed_dserver`); GC and scrub —
    `crates/custodian/src/gc.rs:179-205` (`referenced_fragments` expands
    `0..fragment_count()` through `placed_dserver`, and scrub shares this set);
    reconstruction — `crates/custodian/src/reconstruction.rs:208-232` (`assess`
    expands the same way, and the repair re-commits that expanded vector);
    rebalance — `crates/custodian/src/rebalance.rs:141-167` (`plan_evacuations`
    likewise).
- **Claim:** each new test actually depends on the fallback (it is not a tautology
  that passes regardless).
  - **Test:** the six test files above. Each case was confirmed load-bearing by
    temporarily reverting the consumer it exercises to raw `placement` indexing —
    the empty/short cases then fail (out-of-bounds read, or a committed fragment
    treated as unreferenced/undrained), while the full-placement cases stay green
    because their vectors are already full-length; restoring the shared resolver
    turns them green again. The reconstruction re-placement pin fails if `assess`
    is fed the raw `placement` instead of the expanded vector; the simulation
    scenario fails when only the maintenance side is reverted, proving read and
    maintenance must agree. The whole gate (`cargo xtask ci`) passes.

This is additive, test-only coverage; the behaviour it pins already holds on
`main`, so the tests pass as written and there is no production change to review.

Fixes #349
