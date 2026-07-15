# Build notes — issue 431 / read-block-fault-repair-obligation

## Iteration 2 — what changed vs. iteration 1, and why

The iteration-1 fix was **not rejected on its merits**. The carry-forward records an
*auto-iterate* (round 1): Check "found implementation-level items only, no architectural
judgment required." The two open items were **environment-** and **process-**bound, not
defects in the patch:

1. **Aggregate `cargo xtask ci` (C4-ci) could not be reproduced green in-sandbox.** The
   iteration-1 host blocked loopback bind, so an *unrelated* gRPC test
   (`crates/chunkstore-grpc/tests/list_delete.rs:55`) died with `Operation not permitted`.
   My slice touches only `crates/core/src/read.rs` — nothing in `chunkstore-grpc`.
   **Resolved this run:** on this host that gRPC test passes (`cargo test -p
   wyrd-chunkstore-grpc --test list_delete` → `2 passed`), and the **whole-tree
   `./engine/xtask.sh ci` now exits 0 — "xtask ci: all checks passed"**. The aggregate
   green the previous Check could not reproduce is reproducible here.

2. **T4 prior-art / contribution judgment** — whether closed/rejected *remote* PRs exist
   for this path. This is a human sign-off decision, not a code change. Local history is
   re-confirmed below (§Prior-art); the remote-PR check remains a §9 human item at sign-off.

**I therefore did not change the fix's approach** — doing so would discard an approach
Check accepted on the merits and re-open settled design questions. I re-applied the same
patch cleanly to the current base (`dc503cd`, unchanged from iteration 1), regenerated
`patch.diff` fresh against that base, and re-proved the full red→green + aggregate gate
here. The technical rationale below is carried forward intact.

## What changed, and where (target: `getwyrd/wyrd@main`, base `dc503cd`)

All production changes are in `crates/core/src/read.rs`. Line numbers are the current
target-branch (`main` @ `dc503cd`) pre-patch positions unless noted.

- **`read.rs:150-156`** — new `FaultClass::BlockFault` variant, doc'd as the permanent,
  non-corruption class (mirrors the trichotomy comment at `read.rs:143-148`).
- **`read.rs:169`** — `FaultClass::as_str` → `"block-fault"`.
- **`read.rs:197-199`** — `emit_fragment_fault` gets a dedicated
  `monotonic_counter.read_fragment_block_fault` arm — **not** the corruption counters
  (`read_fragment_corrupt` / `read_fragment_integrity_fault`), satisfying the brief's
  "WITHOUT incrementing the corruption-specific fault signals" leg *by construction* (a
  different counter, not a guard on an existing one).
- **`read.rs:218`** — `read_chunk` gains a second output parameter,
  `block_fault: &mut Vec<ChunkId>`, alongside the existing `corrupt: &mut Vec<ChunkId>`.
- **`read.rs:366-380` → new arm before the catch-all** (RS fan-out). A new match arm is
  inserted **between** the existing `is_integrity_fault` arm (`read.rs:362-365`, the peer
  the brief names) and the final `Err(e) =>` transient arm:
  ```rust
  Err(e) if wyrd_traits::is_block_read_fault(e.as_ref()) => {
      emit_fragment_fault(FaultClass::BlockFault, dserver, frag, &e);
      block_fault.push(chunk.id);
  }
  ```
  The classification uses **the system's single decision point for permanence**,
  `wyrd_traits::is_block_read_fault` (`traits/src/lib.rs:339`) — not re-derived inline,
  exactly as the brief's *Invariant to restore* requires. The final `Err(e) =>` transient
  arm still exists, **unwidened**, for every other non-integrity, non-block-fault error
  (timeouts/unavailable — out of scope per the brief).
- **`read.rs:58-68` (`read_object_from`), `read.rs:75-99` (`read_object_collecting`),
  `read.rs:441-478` (`read_object`), `read.rs:511-528` (`read_chunk_verified`)** — thread
  `block_fault` alongside `corrupt` the same way `corrupt` was already threaded, and after
  the read, drain it onto the same shared queue with a **different** `detected_by`:
  ```rust
  for chunk in block_fault {
      repair::enqueue_repair(meta, chunk, "read-block-fault").await?;
  }
  ```
  (`repair::enqueue_repair`, `crates/core/src/repair.rs:78` — the seam the brief names;
  `detected_by` is documented free-form, `"scrub" | "read"` today.)
- Two existing `#[cfg(test)] mod tests` unit tests in `read.rs`
  (`read_chunk_rejects_a_stored_k_zero_scheme`, `read_chunk_rejects_a_stored_m_zero_scheme`)
  call `read_chunk` directly and needed the extra `&mut block_fault` arg — mechanical, no
  behavioural change (both still assert `corrupt.is_empty()`, unaffected).

`crates/core/tests/read_block_fault_repair.rs` is new (the brief's named test file).

## Why this shape, and what I ruled out

**Why a new `FaultClass::BlockFault` variant instead of reusing `Transient`.** The cheapest
patch that satisfies the *literal* success-criterion text (don't bump
`Corrupt`/`IntegrityFault`) is to route `is_block_read_fault` into the *existing*
`Transient` arm and add the enqueue there — saving ≈13 lines (the new enum variant + its
`as_str` line + its `emit_fragment_fault` counter arm). I rejected it because the brief's
**Invariant to restore** is explicit that a block fault is "permanent damage to read around
AND rebuild, distinct from checksum corruption," and `FaultClass::Transient`'s own
doc-comment (`read.rs:143-145`, left standing three lines above the new code) says "the
D-server may be perfectly healthy a second later" — true for a timeout, **false by
definition** for a dead sector. Tagging a permanent, non-retriable fault with the class
whose contract says "may heal itself" would be internally inconsistent with a doc-comment
this same patch leaves in place, and would make a future "transient faults rising on one
node" operator query count un-healing dead sectors as if they might resolve. The *Invariant
to restore* clause makes correctness, not diff size, the deciding axis (`docs/principles.md`
§1.2) — the new variant is the smallest change that keeps `BlockFault` truthfully distinct
from both siblings the brief says it is `distinct from`.

**Why a second `Vec<ChunkId>` (`block_fault`) instead of retyping `corrupt` to carry a
reason.** Considered collapsing `corrupt: &mut Vec<ChunkId>` into
`&mut Vec<(ChunkId, &'static str)>` and pushing `(chunk.id, "read")` /
`(chunk.id, "read-block-fault")`. Rejected on **measured** cost: that edits every existing
`corrupt.push(chunk.id)` call site (6 of them: `read.rs:222`, `:242`, `:249`, `:342`,
`:346`, `:364`) to add the `"read"` literal, spreading the "which reason" decision into 6
pre-existing push sites on already-passing code. The twin-vec leaves all 6 of those lines
**byte-for-byte unchanged** (verified: `git diff` shows them untouched) and adds exactly one
new push site (`block_fault.push(chunk.id)`) plus one drain loop per caller — a strictly
smaller blast radius for the same behaviour.

**Why the `EcScheme::None` (single-fragment) branch is untouched.** The brief's `Scope`,
`Ordering note`, `Success criterion`, and `Falsifiability` are all phrased for an RS chunk
with "≥ k others remain readable" — a concept that does not exist for a single-fragment
`none`-scheme chunk (nothing to read *around*; a block fault there already propagates as
`Err(e)`, unchanged). Reclassifying that branch is uncited scope expansion.

## Peer callsite opened (per "Citations expected")

`read.rs:362-365` — the `is_integrity_fault` arm the brief names as the composition peer:
enqueue-then-read-around, with a *different* fault class/reason. The new block-fault arm
mirrors its shape (`emit_fragment_fault` + `<vec>.push(chunk.id)`, no early return — the RS
loop keeps consuming `inflight`) but pushes to `block_fault` and drains it with
`"read-block-fault"` instead of `"read"` — preserving exactly the distinction the brief
requires. I also opened `crates/core/tests/read_repair.rs:32-153` (`MemMeta` / `MemChunks` /
`IntegrityFaultingStore`) to clone the harness shape into a block-faulting store, as the
brief's `Citations expected` permits. I did **not** read beyond these two cited callsites.

## Red → green, and the three refutation questions

Verified through the **project's own C4-verify runner**
(`PDCA_BUNDLE=results/issue_431 ./engine/scripts/run-verify.sh`), which applies
`patch.diff` in a dedicated `../wyrd-verify` worktree cut off the bundle's base, runs the
GREEN leg (fix applied), then reverts the production `read.rs` change (keeping the added
test) for the RED leg:
```
run-verify.sh: GREEN — cargo test -p wyrd-core --test read_block_fault_repair (fix applied)
  test result: ok. 1 passed; 0 failed; ...
run-verify.sh: RED — cargo test -p wyrd-core --test read_block_fault_repair (production reverted, test kept)
  assertion `left == right` failed: a permanent block-layer read fault must land a repair
  obligation on the shared queue ...
    left: []
   right: [2970417687]
  test result: FAILED. 0 passed; 1 failed; ...
run-verify.sh: PASS — red without the fix, green with it.
```

- **(a) Genuine red?** **Yes** — the runner reverts the production change and the binding
  queue assertion fails `left: [] / right: [2970417687]` (the RED leg above). The *read*
  still succeeds on the reverted tree (the object reconstructs from the 2 surviving shards);
  it is the *enqueue* the assertion pins, so the test binds the objective, not the read.
- **(b) Production path?** **Yes.** The test calls `wyrd_core::read::read_object` and
  `wyrd_core::repair::{queued_repairs, repair_key}` directly — the exact production entry
  points `crates/core/tests/read_repair.rs` already regresses against, not a copy. The
  `BlockFaultingStore` implements the real `wyrd_traits::ChunkStore`/`PlacementChunkStore`
  and returns a real `wyrd_traits::BlockReadFault` (`traits/src/lib.rs:164-184`); the read
  path's unmodified `wyrd_traits::is_block_read_fault(e.as_ref())` classifier
  (`traits/src/lib.rs:339`) runs against it.
- **(c) Fixture includes the fault?** **Yes.** A real RS(2,1) chunk with 3 real
  erasure-coded fragments (`erasure::encode`); the fixture serves 2 intact and **injects**
  `BlockReadFault` on the 3rd — the fault is present, not curated out. The test proves both
  the read-around (bytes reconstruct byte-identical) and the new enqueue in one run.

## Gates run (commit-readiness)

- **C4-verify** (`./engine/scripts/run-verify.sh`): **PASS** — red→green as above.
- **C4-ci, whole tree** (`./engine/xtask.sh ci` in `$PDCA_WORKTREE`): **PASS —
  "xtask ci: all checks passed", exit 0** (fmt `--check`, clippy `-D warnings`, build,
  test incl. DST, `cargo deny`, conformance). This is the aggregate green iteration-1's
  Check could not reproduce; it reproduces here.
- `cargo fmt -p wyrd-core -- --check` and `cargo fmt --check -- crates/core/tests/read_block_fault_repair.rs`: **clean**.
- `cargo clippy -p wyrd-core --all-targets`: **clean** (no warnings).
- `cargo test -p wyrd-core`: **all green** — 25 lib unit + 1 (new) + 8 `read_repair.rs`
  (the read-path producer contract the brief pins) + 1 + 5 + 3 + 11 + 1 + 2, 0 failures.

## Prior-art (T4, re-confirmed)

`git -C ../wyrd log` over `crates/core/src/read.rs`: `1d2a469` added the final-arm telemetry
and **explicitly deferred this fix** ("#431 owns the block-fault repair question",
`read.rs:368`); `baadd11` handled block faults in the CUSTODIAN read-around, not the
foreground read. Still unfixed on `main` @ `dc503cd` (verified by Read). No equivalent fix
in local merged/all-ref history. The **remote** closed/rejected-PR check for this path is a
human sign-off item (T4 §9) — it cannot be mechanically established from the bundle
artifacts, and is left for the maintainer at sign-off.

## External dependencies

None beyond a working `cargo`/`rustc` toolchain (brief's `External dependencies: none`).
The iteration-1 loopback-bind limitation on the gRPC crate did not recur on this host.
