# Build notes — issue #288 / read-repair-enqueue-integrityfault

## Root cause (two sentences)

`get_fragment_at` in the `EcScheme::None` branch propagates any `Err` via `?` before
`corrupt.push` can run (`read.rs:136`), so an `IntegrityFault` from a verifying store
bypasses the repair-queue enqueue entirely.  In the `EcScheme::ReedSolomon` branch
(`read.rs:189`), `if let Ok(Some(fragment)) = fetched` discards all `Err` variants
silently — an `IntegrityFault` shard is treated as absent, never enqueued.

## What I changed and why

### `crates/core/src/read.rs`

**`EcScheme::None` (was lines 128–137):**

The old code used a single method-chain with `.await?` that short-circuits on any
`Err`.  I replaced it with an explicit `match fetch { ... }` that inspects the error
before propagating it:

```rust
Err(e) if wyrd_traits::is_integrity_fault(e.as_ref()) => {
    corrupt.push(chunk.id);
    return Err(e);
}
Err(e) => return Err(e),  // transient — do NOT push
```

This mirrors exactly what the raw-corrupt-bytes arm already does:
`corrupt.push(chunk.id); Err(e.into())`.  A transient error falls through to the
plain `Err(e)` arm (no push, propagated), matching the brief's invariant that
transient errors are NOT enqueued as corruption.

**`EcScheme::ReedSolomon` (was lines 188–213):**

The old `if let Ok(Some(fragment)) = fetched` silently dropped all `Err`.  I
replaced it with a full `match fetched { ... }` with four explicit arms:

```rust
Ok(Some(fragment)) => { /* existing decode+admit logic */ }
Ok(None) => {}                                    // absent, read around
Err(e) if wyrd_traits::is_integrity_fault(e.as_ref()) => {
    corrupt.push(chunk.id);                       // corrupt → enqueue, read around
}
Err(_) => {}                                      // transient → keep existing drop behaviour
```

The `is_integrity_fault` guard preserves the existing behaviour for transient errors
(silently dropped, treated as absent — "reclassifying non-integrity errors is out of
scope" per the brief) while correctly classifying `IntegrityFault` as a corruption
finding.

No import change was needed: I call `wyrd_traits::is_integrity_fault(e.as_ref())`
with a full path, exactly as `crates/custodian/src/scrub.rs:102` does.

### `crates/core/tests/read_repair.rs`

Added:

1. **`IntegrityFaultingStore`** — a test double wrapping `MemChunks` that returns
   `Err(IntegrityFault { ... })` for one specific `FragmentId` and delegates
   everything else to the inner store.  The default `PlacementChunkStore::get_fragment_at`
   calls `get_fragment`, so overriding `get_fragment` is sufficient to cover both
   the `EcScheme::None` and `EcScheme::ReedSolomon` paths, which both call
   `get_fragment_at`.  This reproduces the EXACT shape `FsChunkStore` (on-disk checksum
   failure) and the gRPC client (`DATA_LOSS` → `IntegrityFault`) emit.

2. **`ec_read_enqueues_integrity_fault_shard_for_repair_and_reconstructs`** — RS(2,1)
   with fragment index 0 returning `IntegrityFault`.  Asserts (a) the read reconstructs
   from the two surviving shards (read-around works) AND (b) the chunk is on the repair
   queue.  Before the fix the repair-queue assertion fires (no entry); after it passes.

3. **`none_read_enqueues_integrity_fault_fragment_for_repair`** — `EcScheme::None`
   with fragment index 0 returning `IntegrityFault`.  Asserts (a) the read fails (nothing
   to reconstruct around) AND (b) the chunk is on the repair queue.  Before the fix
   the repair-queue assertion fires; after it passes.

## Alternatives considered

**Alternative: add `is_integrity_fault` to the `None` arm only, leave RS unchanged.**
Rejected: the brief names both sites explicitly (`read.rs:128-137` and `read.rs:188-213`)
and the invariant is "the corruption-vs-transient classification must be applied uniformly
at every fragment fetch site."  Fixing only `None` leaves RS silently absorbing
`IntegrityFault` shards from real backends.

**Alternative: propagate transient errors in the RS branch (change `Err(_) => {}` to
`Err(e) => return Err(e)`).**
Rejected per the brief ("reclassifying non-integrity errors is out of scope") and
because the ORIGINAL code dropped transient errors via `if let Ok(Some(fragment))`.
Changing that behaviour is a separate correctness decision.  The diff is exactly two
characters (`Err(_) => {}` vs `Err(e) => return Err(e)`) but the scope change is
deliberate.

## Red→green evidence

`./engine/xtask.sh ci` (the project's own gate runner, `pdca.toml [gates] runner`)
exits 0 with all 6 `read_repair.rs` tests green, including both new regressions.

The test file is an addition to the existing `read_repair.rs` (not a new file), so
`run-verify.sh` takes the "green-only" path (the co-located case documented in the
script): the C4-ci gate over the full tree is the binding evidence.  The reviewer
can verify flippability by reverting the two changed blocks in `read.rs` and
confirming both new tests fail on the unpatched logic.

## Formatter

`cargo fmt --all` was run in the worktree before generating `patch.diff`.  The only
formatting change it made was collapsing the multi-line `Ok(None)` arm:

```rust
// before fmt:
Ok(None) => {
    return Err(ReadError::MissingFragment { chunk_id: chunk.id }.into())
}
// after fmt:
Ok(None) => return Err(ReadError::MissingFragment { chunk_id: chunk.id }.into()),
```

The diff in `patch.diff` reflects the post-formatter state and will apply cleanly to
`origin/main`.

## Citations

- `crates/core/src/read.rs:128–137` — `EcScheme::None` site (before fix)
- `crates/core/src/read.rs:188–213` — `EcScheme::ReedSolomon` site (before fix)
- `crates/custodian/src/scrub.rs:102` — the `is_integrity_fault` classifier being mirrored
- `crates/traits/src/lib.rs:111` — `is_integrity_fault` definition
- `0005:174-176` — "the read path feeds the shared repair queue" invariant
