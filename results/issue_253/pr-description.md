# Map a lost TiKV write-write race to Conflict, not a fault

## Summary

When two clients concurrently updated the **same** metadata key through the TiKV
backend, the writer that lost the race got back a hard error (`Err`) instead of a
`Conflict`. A caller that correctly handles a conflict — re-read and retry — was
instead forced to treat an ordinary concurrent update as a backend failure, so
routine contention looked like an outage. This change makes a losing writer return
`Ok(CommitOutcome::Conflict)` while genuine faults still return `Err`, restoring the
distinction the `MetadataStore` contract promises.

## What to look at

- `crates/metadata-tikv/src/lib.rs` — the two new helpers `is_write_conflict`
  (classifies the error) and `conflict_or_err` (rolls back, then maps), and the two
  `commit()` arms now routed through them: the `get_for_update` precondition-lock arm
  and the final `txn.commit()` arm.
- `crates/metadata-tikv/tests/contention.rs` (new) — the two race tests; the crux is
  the outcome tally (`committed == 1`, `conflicts == WRITERS - 1`, panic on any `Err`).
- The trait in `crates/traits/src/lib.rs` is deliberately **not** touched.

To exercise it: `cargo xtask tikv-conformance` brings up the throwaway single-node
TiKV, rebuilds with `--features tikv`, and runs both the `conformance` and `contention`
binaries. On a machine with no TiKV the contention tests skip cleanly and
`cargo xtask ci` stays green.

## Root cause

`commit()` sent every backend error through a single rollback-then-propagate path that
converts all errors to `Err`. But under a pessimistic transaction a real write-write
conflict is a normal outcome that TiKV reports as an error — at `get_for_update` (lock
time) or at `commit()` (prewrite) — so the losing writer was mislabelled a fault
instead of a conflict.

## Fix

`commit()` now classifies the error on those two arms. `is_write_conflict` returns true
only for a TiKV `KeyError` carrying write-conflict information, recursing through the
batched (`MultipleKeyErrors` / `ExtractedErrors`) and pessimistic-lock wrappers it can
arrive inside; every other error kind (network, region-unavailable, PD timeout, lock
resolution, deadlock, undetermined) stays `Err`. `conflict_or_err` best-effort rolls the
transaction back — releasing any prewrite locks promptly and keeping the transaction
drop-safe — then returns `Ok(Conflict)` for a race and `Err` for a fault. The
precondition-miss cleanup rollback is made best-effort too, so a cleanup hiccup can't
mask the legitimate `Conflict`. `put`/`delete` keep propagating errors as `Err` (they
buffer locally and cannot raise a write-conflict).

## Verification

- **Claim:** a forced write-write race surfaces as exactly one `Ok(Committed)` and the
  rest `Ok(Conflict)`, with **zero `Err`**, and the final stored value equals the
  winner's write; a genuine fault still surfaces as `Err`.
  - **Checked:** `crates/metadata-tikv/src/lib.rs:277,306-307` — both the
    `get_for_update` and `txn.commit()` error arms route through `conflict_or_err`;
    `crates/metadata-tikv/src/lib.rs:149-160` — `is_write_conflict` folds *only* a
    `KeyError` with conflict info, so faults stay faults.
  - **Test:** `crates/metadata-tikv/tests/contention.rs` — `write_write_race_exactly_one_winner`
    and `require_absent_race` fan eight independent connections at one shared key and
    assert `committed == 1`, `conflicts == WRITERS - 1`, panic on any `Err`
    (`tests/contention.rs:162,165-169`), and final value == winner. Against the pre-fix
    `commit()` the losers return `Err`, so the "zero `Err`" assertion **fails** (red);
    with the classification it **passes** (green).
- **Claim:** the contract's `Conflict`-vs-fault partition is the one being honoured.
  - **Checked:** `crates/traits/src/lib.rs:353-361` — `CommitOutcome::{Committed,Conflict}`;
    `Conflict` (not `Err`) is the documented outcome for a rejected writer.
- **Claim:** the change is additive to the gate and safe without a live cluster.
  - **Checked:** the contention suite is endpoint-gated and skips cleanly with no
    `WYRD_TIKV_PD_ENDPOINTS`; `cargo xtask ci` (default features) stays green.
    `cargo xtask tikv-conformance` now runs both the `conformance` and `contention`
    binaries — `xtask/src/main.rs:194-202`.

The behavioural red→green is only observable against a live cluster (the tests skip
without one); the run against the throwaway single-node TiKV shows one `Committed`,
seven `Conflict`, zero `Err`, final value == winner.

Fixes [#253](https://github.com/getwyrd/wyrd/issues/253)
