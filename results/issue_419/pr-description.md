## Summary

The metadata store promises a specific read-consistency behaviour — a read
always sees the most recently committed value, a mutation that lands between a
read-then-commit's read and its own commit loses cleanly instead of tearing or
duplicating the binding, and a single listing reflects one consistent snapshot.
Until now that promise lived only in prose (ADR-0015 clause 3); no test
enforced it, so a backend could quietly regress any of the three and still pass
the suite. This adds three backend-agnostic conformance properties that pin the
behaviour and run against the redb store, each proven to catch a real violation
([#419]). Test and suite code only — no production behaviour changes.

[#419]: https://github.com/getwyrd/wyrd/issues/419

## What to look at

- **`crates/metadata-conformance/src/lib.rs`** — the three new properties on the
  shared `MetadataStore` trait: `contract_read_after_commit` (`:136-154`),
  `contract_rename_race_yields_conflict` (`:167-230`, modelling the rename
  read → `require` re-check → commit shape from `crates/core/src/metadata.rs`),
  and `contract_scan_is_consistent_cut` (`:244-280`). They use only the trait
  surface, so any backend driving the suite inherits them unchanged.
- **`crates/metadata-conformance/tests/demonstrated_red.rs`** — the crux of the
  review. For each property a deliberately-broken in-memory store proves the
  property is load-bearing: the property goes red against it, while the four
  pre-existing sequential contracts still pass it (so each property is shown to
  add coverage the suite lacked, not restate an existing check).
- **`crates/metadata-redb/tests/conformance.rs:37-39`** — wires all three into
  the redb driver.
- **Note on scan:** redb's `scan` is a single atomic local read, so
  `contract_scan_is_consistent_cut` passes trivially there; its value is the
  documented baseline the counter-store below shows is non-trivial, ready for a
  paged backend where an inconsistent cut can actually occur.
- **Reproduce:** `cargo test -p wyrd-metadata-redb --test conformance` (the
  contract suite, now seven properties) and `cargo test -p
  wyrd-metadata-conformance --test demonstrated_red` (the counter-store proofs).

## Root cause

The decided read-consistency level was documented but had no executable pin: the
existing four `contract_*` functions cover only sequential behaviour and never
exercise repeated overwrites, an interleaved mutation, or a listing across a
rename. A stale-reading, precondition-skipping, or index-leaking backend would
therefore pass the suite unnoticed.

## Fix

Add three properties to the shared conformance crate covering the
snapshot/temporal dimension, drive them from the redb conformance test, and add
a dev/test-only harness of three broken stores that demonstrates each property
catches a distinct failure the existing suite misses. No file under
`metadata-redb/src`, `metadata-tikv/src`, `core`, or `traits` is touched.

## Verification

- **Claim:** a read observes the most recently committed value across repeated
  overwrites (read-your-writes / anti-stale-read).
  - **Checked:** `crates/metadata-conformance/src/lib.rs:136-154`, run on redb
    via `crates/metadata-redb/tests/conformance.rs:37`.
  - **Test:** `crates/metadata-conformance/tests/demonstrated_red.rs:95-98` — a
    store that pins a key's value after its second write makes the property go
    red, while `:103` shows that same store still passes the existing sequential
    contracts.

- **Claim:** a mutation landing between a read-then-commit's read and its commit
  yields a conflict, never a torn or duplicated binding.
  - **Checked:** `crates/metadata-conformance/src/lib.rs:167-230`, run on redb
    via `crates/metadata-redb/tests/conformance.rs:38`.
  - **Test:** `crates/metadata-conformance/tests/demonstrated_red.rs:167-168` — a
    store that skips a `require` precondition on a key it also deletes lets the
    stale writer through (duplicating the binding) and the property catches it,
    while `:175` shows the existing contracts do not.

- **Claim:** a single listing observes one consistent cut across a rename — the
  moved entry appears in exactly one position, never both, never neither.
  - **Checked:** `crates/metadata-conformance/src/lib.rs:244-280`, run on redb
    via `crates/metadata-redb/tests/conformance.rs:39`.
  - **Test:** `crates/metadata-conformance/tests/demonstrated_red.rs:248-249` — a
    store whose listing index leaks deleted keys returns both positions and the
    property catches it, while `:256` shows the existing scan contract does not.

- **Claim:** no production behaviour changes; the whole gate is green.
  - **Checked:** only `crates/metadata-conformance/{src/lib.rs,Cargo.toml,tests/
    demonstrated_red.rs}` and `crates/metadata-redb/tests/conformance.rs` change.
  - **Test:** `cargo xtask ci` (fmt, clippy, build, workspace tests including the
    new properties and the counter-store proofs, deny, conformance vectors)
    exits 0.

Fixes #419
