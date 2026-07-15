# Build notes — issue 431 / read-block-fault-repair-obligation (iteration 4)

## What the defect is
The RS foreground read fan-out (`crates/core/src/read.rs`) reads *around* a permanent
block-layer read fault on one shard when ≥ k others survive, returns the object, and
enqueues **no** repair obligation. Every non-integrity fetch error fell into the final
`Err(e) =>` arm and was emitted as `FaultClass::Transient` with no enqueue — the arm's own
comment said "#431 owns the block-fault repair question". Yet `wyrd_traits::BlockReadFault`
is documented PERMANENT damage (`traits/src/lib.rs:164-199`) and the custodian already
treats it as a permanent read fault on the rebuild side (`reconstruction.rs:475`). Result:
silent, avoidable durability debt until a later scrub trips over the same fault.

## The fix (one logical change, `crates/core/src/read.rs`)
Add a dedicated match arm **before** the transient catch-all in the RS fan-out that
recognises the block fault through the system's single decision point for permanence —
`wyrd_traits::is_block_read_fault` (`traits/src/lib.rs:339`), not re-derived inline — reads
around it exactly as any other excluded shard, and records the chunk on a new `block_fault`
finding vec. That vec is drained onto the SAME shared repair queue (`repair::enqueue_repair`)
that corruption findings use, but with a distinct non-corruption `detected_by = "read-block-fault"`
reason, so the audit trail can tell a dead sector from a checksum failure. A new
`FaultClass::BlockFault` gets its own telemetry counter (`read_fragment_block_fault`) so the
corruption-specific counters (`Corrupt` / `IntegrityFault`) are untouched — nothing was ever
mis-read, so this must not bump the corruption metrics.

Threading: `block_fault: &mut Vec<ChunkId>` is added alongside the existing `corrupt` vec
through `read_object_from` (66), `read_object_collecting` (78-93), `read_chunk` (signature),
and drained at the two enqueue sites `read_object` and `read_chunk_verified`. The two
in-file unit-test callsites of `read_chunk` were updated to pass the new argument.

Citations on the target (worktree off `origin/main`, post-fmt line numbers):
- new match arm: `crates/core/src/read.rs:395-410` (mirrors the integrity arm at ~:378-381
  but with `FaultClass::BlockFault` and a distinct reason)
- `FaultClass::BlockFault` variant + `as_str` + counter: `read.rs:150-156`, `:163`, `:198-200`
- `read_object` drain: `read.rs:471-476` (`"read-block-fault"`, enqueue at `:475`)
- `read_chunk_verified` drain: `read.rs:522-527` (enqueue at `:526`)
- decision point used, not re-derived: `wyrd_traits::is_block_read_fault`, `traits/src/lib.rs:339`

## Why this shape (invariant, not diff-size)
The brief names an **Invariant to restore**: durable damage detected at read time is never
absorbed silently; the read path feeds the SAME queue scrub feeds, and permanence is decided
by `is_block_read_fault` — the single decision point — not re-derived. The chosen change is
the smallest one that restores that invariant while preserving the existing distinction that
block faults are *not* checksum corruption (own counter, own `detected_by`). Alternatives
ruled out:
- Reusing `corrupt` + `"read"` reason: would conflate a dead sector with bit rot, bumping the
  corruption metric — violates the Success criterion's "WITHOUT incrementing the
  corruption-specific fault signals" and loses the audit distinction.
- Re-deriving permanence inline (matching `BlockReadFault`/EIO in read.rs): duplicates the
  `traits/src/lib.rs:325-350` classifier and lets the two drift — the brief forbids it.
- Reclassifying other non-integrity errors (timeouts/unavailable): out of scope; those stay
  transient and un-enqueued (`read.rs`'s telemetry-only handling stands).

## Test — `crates/core/tests/read_block_fault_repair.rs` (NEW file)
RS(2,1) object over a `BlockFaultingStore` test-double whose `get_fragment` returns
`Err(BlockReadFault)` for fragment index 0 while indices 1,2 serve intact fragments (k=2
survive). Asserts: (1) `read::read_object` still returns the correct bytes; (2)
`repair::queued_repairs` now contains the chunk; (3) reading the repair key's VALUE back
through the `MetadataStore` yields `detected_by = "read-block-fault"` (≠ the corruption
producers' `"read"`). Harness shape cloned from `crates/core/tests/read_repair.rs:32-153`
(`MemMeta`/`MemChunks`), with the block-faulting store substituted for the integrity-faulting
one — the composition peer the brief cited.

## Refute-your-own-test (forced check)
- **(a) Genuine red?** YES. C4-verify's RED leg reverts the `read.rs` production change,
  keeps the test, and the binding queue assertion fails: `left: []  right: [2970417687]`
  (`read_block_fault_repair.rs:260`). Without the fix the queue stays empty.
- **(b) Production path?** YES. The test calls `wyrd_core::read::read_object`, the production
  entry the fix changes — no copy/mock of the read logic. Only the `ChunkStore` backend is a
  test double (as the cited peer does), which is the seam a real dead sector arrives through.
- **(c) Fixture includes the fault?** YES. `BlockFaultingStore` actually injects
  `BlockReadFault::new(id, ...)` for the specific faulting fragment; the other two are real
  intact fragments encoded by `erasure::encode`. The fault is present, not curated out.

## Verification run (project runner)
`PDCA_BUNDLE=results/issue_431 ./engine/scripts/run-verify.sh` →
`PASS — red without the fix, green with it.` GREEN leg: 1 passed. RED leg: 1 failed on the
queue assertion. Additionally: `cargo test -p wyrd-core --lib --no-run` compiles (the two
touched in-file unit tests build), `cargo clippy -p wyrd-core --tests` clean, and
`cargo fmt -p wyrd-core -- --check` clean — commit-ready for the target's hooks.

## Carry-forward from iterations 1–3 (why this is not the "rejected approach unchanged")
The three prior auto-iterates did NOT reject the fix on merit — the focused red→green passed
each time. Both recurring sign-off items are environmental / evidentiary, outside the
builder's control:

1. **Aggregate `cargo xtask ci` (C4-ci) could not be reproduced in-sandbox.** An UNRELATED
   gRPC test (`crates/chunkstore-grpc/tests/list_delete.rs:55`) fails with
   `Operation not permitted` because this sandbox cannot bind loopback. That is not caused by
   this patch (it touches only `crates/core`), and clippy above compiles the grpc crate fine —
   only the *runtime* loopback bind is blocked. This is a genuine environment dependency the
   plan did not register. See the NEEDS-HUMAN marker below; the aggregate C4-ci must be run on
   a host permitted to bind loopback to confirm the whole-tree green.

2. **T4 remote prior-art** (closed/rejected PRs for `crates/core/src/read.rs`) cannot be
   established mechanically from the supplied artifacts — local history shows only the
   telemetry predecessor (1d2a469) that explicitly deferred this fix, no equivalent block-fault
   fix. This is a human sign-off judgement (T4), not a builder-resolvable gate.

Neither is a reason to alter the approach, and the fix itself is unchanged in substance from
the sound v3 patch because v3 was never faulted on substance — it was parked on the two items
above. The patch was re-derived cleanly against today's `origin/main` base (worktree tip
`dc503cd`) and re-verified red→green here.

NEEDS-HUMAN external dependency: loopback-bind (host network sandbox) — blocks the aggregate
`cargo xtask ci` / C4-ci runtime leg (an unrelated gRPC test at
`crates/chunkstore-grpc/tests/list_delete.rs:55` fails `Operation not permitted` when it
cannot bind loopback); the per-fix C4-verify red→green for THIS patch passed independently,
so this does not gate the fix, but the whole-tree green must be confirmed on a host permitted
to bind loopback.

```toml
[[doctor.checks]]
id    = "loopback-bind"   # a runtime gRPC test binds a loopback listener
cmd   = "python3 -c \"import socket; s=socket.socket(); s.bind(('127.0.0.1',0)); s.close()\""
hint  = "run the cycle on a host/sandbox that permits binding a 127.0.0.1 port (chunkstore-grpc integration tests bind loopback); grant loopback in the sandbox profile"
level = "WARN"           # only the grpc runtime leg degrades; core builds/tests fine without it
```
