# Give chunk placement expansion a single definition

## Summary

Three maintenance loops that walk a chunk's fragments out to the D-servers
holding them — garbage collection, reconstruction, and rebalance — each carried
their own copy of the same "resolve every fragment index, applying the
identity-placement fallback for pre-M3 records" logic. When one copy drifted, a
live fragment of a mixed-era chunk on a draining D-server was silently skipped,
and there was no single definition to catch it (that specific divergence was the
rebalance bug fixed in commit `04ad01aab4bdeefe29e429f84610abf1b963e1d3`). This
change introduces one helper, `ChunkRef::fragments()`, as the single
placement-expansion definition and routes all three loops through it. No
behaviour changes.

## What to look at

- **The new helper:** `ChunkRef::fragments()` in `crates/core/src/metadata.rs`,
  added directly after `placed_dserver`. It returns the full
  `0..fragment_count()` index space as `(fragment index, holding D-server)`
  pairs and delegates to `placed_dserver` per index — it does not re-read the
  raw `placement` vector, so it resolves identically to the read path by
  construction.
- **The three one-line routing swaps** that replace an open-coded
  `(0..fragment_count()).map(|i| placed_dserver(i))` walk with `chunk.fragments()`:
  `referenced_fragments` (`crates/custodian/src/gc.rs`), `assess`
  (`crates/custodian/src/reconstruction.rs`), and `plan_evacuations`
  (`crates/custodian/src/rebalance.rs`).
- **How to exercise it:** `cargo test -p wyrd-core --test placement_record`
  covers the helper directly; `cargo test -p wyrd-custodian` confirms the three
  rewritten loops behave as before.

This follows ADR-0040 (decisions 1 & 2): one normative placement-expansion rule,
one helper (`fragments()`) built on the `fragment_count()` / `placed_dserver()`
primitives, deliberately liberal (infallible, applies the identity fallback
unconditionally, does not validate length).

## Root cause

The `(fragment index, holding D-server)` set of a committed chunk map had no
single definition — GC, reconstruction, and rebalance each open-coded the same
expansion, so the copies could disagree without any check noticing. That is
exactly how the rebalance copy came to iterate the raw `placement` vector and
skip live fragments of pre-M3 chunks.

## Fix

Add `ChunkRef::fragments()` as the one expansion helper, delegating to
`placed_dserver` per index, and route GC's `referenced_fragments`,
reconstruction's `assess`, and rebalance's `plan_evacuations` through it. The
`placed_dserver` doc comment is updated to list rebalance among its callers.
Because the helper returns exactly what the three loops previously computed, the
change is a pure centralization — the resolved placement each loop consumes is
unchanged.

## Verification

- **Claim:** The three read-side consumers now resolve their fragment→D-server
  expansion through one definition, not three copies.
  - **Checked:** on `main` the identical open-coded walk lives at
    `crates/custodian/src/gc.rs:197-200`,
    `crates/custodian/src/reconstruction.rs:230-232`, and
    `crates/custodian/src/rebalance.rs:165-167`; this change replaces each with
    `chunk.fragments()`.
- **Claim:** The helper cannot disagree with the read path's resolution.
  - **Checked:** `fragments()` delegates to `placed_dserver`
    (`crates/core/src/metadata.rs:119-124`) over `0..fragment_count()`
    (`crates/core/src/metadata.rs:103-108`) rather than reading the raw
    `placement` vector, so it is identity-fallback-equivalent by construction —
    the same reason the divergence in
    `04ad01aab4bdeefe29e429f84610abf1b963e1d3` becomes unreachable here.
- **Claim:** Behaviour is preserved.
  - **Checked:** the existing custodian GC / rebalance / reconstruction test
    suites pass unchanged, and `cargo xtask ci` (fmt, clippy `-D warnings`,
    build, test incl. DST, deny, conformance) is green.
- **Test:** `crates/core/tests/placement_record.rs` (new `fragments_matrix`)
  asserts `fragments()` yields exactly the per-index `placed_dserver` resolution
  for `EcScheme::None` and `ReedSolomon { k, m }` across empty, full
  (`len == fragment_count()`), and short placement vectors. It is red pre-fix
  (the helper does not exist, so the file fails to compile) and green post-fix.

Fixes #347
