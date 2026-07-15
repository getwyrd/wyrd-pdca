# Build notes — issue 431 / read-block-fault-repair-obligation

## What changed, and where (target: `getwyrd/wyrd@main`, base `dc503cd`)

All production changes are in `crates/core/src/read.rs`. Diff summary (see
`patch.diff` for the full unified diff):

- `read.rs:150-156` — new `FaultClass::BlockFault` variant, doc'd as the permanent,
  non-corruption class (mirrors the trichotomy comment at `read.rs:130-134`).
- `read.rs:169` — `FaultClass::as_str` → `"block-fault"`.
- `read.rs:197-199` — `emit_fragment_fault` gets a dedicated
  `monotonic_counter.read_fragment_block_fault` arm — **not** the corruption counters
  (`read_fragment_corrupt` / `read_fragment_integrity_fault`, `read.rs:178-186`
  pre-patch), satisfying the brief's "WITHOUT incrementing the corruption-specific
  fault signals" leg by construction (a different counter, not a guard on an existing
  one).
- `read.rs:218` — `read_chunk` gains a second output parameter, `block_fault: &mut
  Vec<ChunkId>`, alongside the existing `corrupt: &mut Vec<ChunkId>`.
- `read.rs:382-399` (RS fan-out, was `read.rs:362-380` pre-patch) — a new match arm
  **before** the final catch-all:
  ```rust
  Err(e) if wyrd_traits::is_block_read_fault(e.as_ref()) => {
      emit_fragment_fault(FaultClass::BlockFault, dserver, frag, &e);
      block_fault.push(chunk.id);
  }
  ```
  placed between the existing `is_integrity_fault` arm (`read.rs:377-380`, the peer
  cited by the brief, unchanged in shape — enqueue + read-around, different class) and
  the final `Err(e) =>` transient arm, which still exists, unwidened, for every other
  non-integrity, non-block-fault error (timeouts/unavailable — out of scope per the
  brief).
- `read.rs:75-99` (`read_object_collecting`), `read.rs:441-478` (`read_object`),
  `read.rs:511-528` (`read_chunk_verified`), `read.rs:58-68` (`read_object_from`) —
  thread `block_fault` alongside `corrupt` the same way `corrupt` was already
  threaded, and after the read, drain it onto the same queue with a **different**
  `detected_by`:
  ```rust
  for chunk in block_fault {
      repair::enqueue_repair(meta, chunk, "read-block-fault").await?;
  }
  ```
  (`repair::enqueue_repair`, `crates/core/src/repair.rs:78`, the seam the brief names —
  `detected_by` is documented free-form, `repair.rs:76`.)
- Two existing unit tests in `read.rs`'s `#[cfg(test)] mod tests`
  (`read_chunk_rejects_a_stored_k_zero_scheme` at `read.rs:732`,
  `read_chunk_rejects_a_stored_m_zero_scheme` at `read.rs:775`, pre-patch line
  numbers) call `read_chunk` directly and needed the extra `&mut block_fault` arg —
  mechanical, no behavioural change (both assert `corrupt.is_empty()`, unaffected).

`crates/core/tests/read_block_fault_repair.rs` is new (the brief's named test file).

## Why this shape, and what I ruled out

**Why a new `FaultClass::BlockFault` variant instead of reusing `Transient`.** The
cheapest patch that would satisfy the *literal* success-criterion text (don't bump
`Corrupt`/`IntegrityFault`) is to route `is_block_read_fault` into the *existing*
`Transient` arm and just add the enqueue there — saves the ~13 lines of new enum
variant + counter arm. I rejected it because the brief's **Invariant to restore**
is explicit that a block fault is "permanent damage to read around AND rebuild,
distinct from checksum corruption" and `wyrd_traits::BlockReadFault`'s own doc
(`traits/src/lib.rs:150-151`, cited by the brief) says a consumer "must not... schedule
a checksum-repair" — i.e. it is not merely non-corruption, it is a *different
permanence class* from `Transient` too: `FaultClass::Transient`'s own doc-comment
(`read.rs:146-148`, unchanged by this patch) says "the D-server may be perfectly
healthy a second later" — true for a timeout, false by definition for a dead sector.
Tagging a permanent, non-retriable fault with the class whose contract says "may heal
itself" would be internally inconsistent with a doc-comment this same patch leaves
standing three lines above the new code, and would make a future operator-facing
"transient faults rising on one node" query (the stated purpose of per-class counters,
`read.rs:169-171`) count un-healing dead sectors as if they might resolve. The
`Invariant to restore` clause makes correctness, not diff size, the deciding axis here
(`docs/principles.md` §1.2 cited by the harness) — the new variant is the smallest
change that keeps `BlockFault` truthfully distinct from both siblings it's `distinct
from` per the brief.

**Why a second `Vec<ChunkId>` parameter (`block_fault`) instead of retyping
`corrupt` to carry a reason.** Considered collapsing `corrupt: &mut Vec<ChunkId>`
into `corrupt: &mut Vec<(ChunkId, &'static str)>` and pushing `(chunk.id, "read")` /
`(chunk.id, "read-block-fault")` from the same vec, then grouping by reason once
before the two enqueue loops. Rejected on cost: that touches every existing
`corrupt.push(chunk.id)` call site (6 of them: `read.rs:222`, `:242`, `:249`, `:342`,
`:346`, `:364` pre-patch) to add the literal `"read"`, plus the two existing unit
tests' `corrupt.is_empty()` assertions still work unchanged either way but the type
itself changes across every signature `corrupt` appears in (`read_chunk`,
`read_object_collecting`, `read_object`, `read_chunk_verified` — same 4 functions
either approach touches), so line count is comparable, but a tuple type spreads the
"which reason" decision into 6 pre-existing push sites instead of the 1 new one this
patch actually adds — a strictly larger blast radius on already-passing code for the
same behavioural result. The twin-vec keeps every existing `corrupt.push(chunk.id)`
line **untouched** (verified: `git diff` shows those 6 lines unchanged) and adds
exactly one new push site (`block_fault.push(chunk.id)`, the new match arm) plus one
new drain loop per caller — a smaller, more localized diff for the same outcome.

**Why the `EcScheme::None` (single-fragment) branch is untouched.** The brief's
`Scope` bullet and `Ordering note` both point at "the RS fan-out match... `~:306-380`"
specifically; the `Success criterion` and `Falsifiability` are phrased in terms of an
RS chunk with "≥ k others remain readable" — a concept that doesn't exist for a
single-fragment `none`-scheme chunk (there is nothing to read *around*; a block fault
there already propagates as `Err(e)` at `read.rs:227`, unchanged, exactly as an
integrity fault there is handled by a *different*, still-untouched arm at
`read.rs:221-224`). Reclassifying that branch's non-integrity error handling is not
named anywhere in the brief and would be an uncited scope expansion.

## Peer callsite opened (per "Citations expected")

`read.rs:362-365` (pre-patch numbering) — the `is_integrity_fault` arm the brief names
as the composition peer to mirror: enqueue-then-read-around, but with a *different*
fault class/reason. The new block-fault arm mirrors its shape (`emit_fragment_fault`
+ `<vec>.push(chunk.id)`, no early return — the RS loop keeps consuming
`inflight`) but pushes to `block_fault` instead of `corrupt`, and drains
`block_fault` with `"read-block-fault"` instead of `"read"` — preserving exactly the
distinction the brief requires ("preserving the existing distinction that block faults
are not checksum corruption").

I did **not** open `crates/core/src/repair.rs` beyond what the brief itself quotes
inline (`enqueue_repair`'s signature and doc, `repair.rs:73-86`, already reproduced in
the brief text) — no new callsite needed there; `detected_by` is already documented
free-form.

## Red → green, and the three refutation questions

**(a) Genuine red?** Yes — verified twice:
1. `git stash push -- crates/core/src/read.rs` (revert production, keep the new test)
   → `cargo test -p wyrd-core --test read_block_fault_repair` fails:
   ```
   assertion `left == right` failed: a permanent block-layer read fault must land a
   repair obligation on the shared queue, ...
     left: []
    right: [2970417687]
   ```
   then `git stash pop` restores the fix and the same test passes.
2. The project's own C4-verify gate (`./engine/scripts/run-verify.sh`, run with
   `PDCA_BUNDLE=results/issue_431`) does the same revert/reapply in an isolated
   worktree and reports:
   ```
   run-verify.sh: GREEN — cargo test -p wyrd-core --test read_block_fault_repair (fix applied)
   test result: ok. 1 passed; 0 failed; ...
   run-verify.sh: RED — cargo test -p wyrd-core --test read_block_fault_repair (production reverted, test kept)
   test result: FAILED. 0 passed; 1 failed; ...
   run-verify.sh: PASS — red without the fix, green with it.
   ```

**(b) Production path?** Yes. The test calls `wyrd_core::read::read_object` and
`wyrd_core::repair::{queued_repairs, repair_key}` directly — the exact production
entry points `crates/core/tests/read_repair.rs` (the brief's cited existing-suite
peer) already regressions against, not a copy. The `BlockFaultingStore` test double
implements the real `wyrd_traits::ChunkStore`/`PlacementChunkStore` traits and
constructs a real `wyrd_traits::BlockReadFault` (`traits/src/lib.rs:164-199`), the
same seam type a live `chunkstore-fs`/gRPC backend would surface — the read path's
`wyrd_traits::is_block_read_fault(e.as_ref())` classifier runs unmodified against it.

**(c) Fixture includes the fault?** Yes. The commissioned inode is a real RS(2,1)
chunk with 3 real erasure-coded fragments (`erasure::encode`); the fixture serves 2
intact and fails the 3rd with `BlockReadFault` — the fault is injected, not curated
out (contrast a `healthy_fleet`-style fixture that would just omit the bad fragment
entirely). The test proves both the read-around (bytes still reconstruct) and the
new enqueue in the same run.

## Test-runner note

Ran via `cargo test -p wyrd-core --test read_block_fault_repair` directly (plain
`#[tokio::test]`, no cfg gate, matching the brief's `Falsifiability` note) during
development, and via the project's `C4-verify` wrapper
(`./engine/scripts/run-verify.sh`, invoked with `PDCA_BUNDLE`/`BUNDLE` set to this
bundle dir, no hand-rolled container/timeout-free invocation) for the authoritative
red→green determination reproduced above. Also ran the full `cargo test -p wyrd-core`
(lib unit tests + all `tests/*.rs`, 25 + 8 + 5 + 3 + 11 + 1 + 2 = all passing) and
`cargo fmt -p wyrd-core -- --check` / `cargo clippy -p wyrd-core --all-targets`
(both clean) to confirm nothing else in the crate regressed and the patch is
commit-ready for the target's own formatter/lints.

## External dependencies

None beyond a working `cargo`/`rustc` toolchain, already assumed by the runner
(brief's `External dependencies: none`).
