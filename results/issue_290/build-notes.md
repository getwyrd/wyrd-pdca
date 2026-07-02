# Build notes — issue 290 / no-preallocate-from-untrusted-inode-size

## What changed

`crates/core/src/read.rs:79` (pre-fix, target `main` @ `b91401a`):

```rust
let mut bytes = Vec::with_capacity(inode.size as usize);
```

`inode.size` is decoded straight from stored JSON (`metadata::decode`, no bound
checks) before `read_object_collecting` ever touches the chunk map. A committed
`InodeRecord { size: u64::MAX, chunk_map: vec![], .. }` panics here:
`Vec::with_capacity` requires the byte length to fit `isize::MAX`; `u64::MAX`
doesn't, so it hits Rust's "capacity overflow" panic in
`alloc::raw_vec` — confirmed locally (see "Red→green proof" below). Even a
smaller-but-still-oversized value that *does* fit `isize::MAX` would still drive
an eager multi-GB allocation before the size check at `read.rs:83-89` ever runs.

Fix (`read.rs:79-86` post-fix):

```rust
let mut bytes = Vec::new();
for chunk in &inode.chunk_map {
    bytes.extend_from_slice(&read_chunk(chunks, chunk, corrupt).await?);
}
```

Dropping the `with_capacity` hint entirely means the buffer now grows only from
bytes `read_chunk` has *already fetched and checksum-verified* per chunk
(`read.rs:120-181` — `read_chunk` decodes/verifies each fragment before
returning bytes to the caller). `inode.size` is no longer consulted until the
post-hoc equality check at `read.rs:90-96` (`ReadError::SizeMismatch`), which is
exactly the invariant the brief names: "reassembly allocates from what the
chunk map can commit, not from the recorded size."

This is the smallest change that restores the invariant (a one-line diff to the
allocation site) — the brief names an **Invariant to restore**, so minimality of
diff was not the deciding axis; restoring the invariant was, and it happens to
also be the smallest correct change (`docs/principles.md` §1.2, §2 per the
task framing).

## Alternatives considered and ruled out

1. **Cap the capacity hint at some fixed ceiling** (e.g.
   `inode.size.min(SOME_MAX) as usize`) before falling through to the same
   growth-on-read behavior for anything larger. Rejected: the brief's scope
   explicitly excludes "configurable object/read size limits as a new feature"
   (brief.md `Scope / out of scope`), and picking an arbitrary hardcoded ceiling
   invents exactly that policy without any spec backing a number. It also still
   allows a smaller-but-plausible lie (e.g. `size` a few MB larger than what the
   chunk map can back) to pointlessly over-allocate up to the cap before the
   mismatch check catches it — no correctness benefit over just not
   pre-allocating from the untrusted field, only complexity.

2. **Sum `ChunkRef.len` across `chunk_map` and use that as the capacity hint**
   instead of `inode.size`. Considered because the brief's own phrasing ("what
   the chunk map can commit") could be read as "size the buffer off the chunk
   map's *declared* lengths." Rejected on inspection: `ChunkRef.len` (`
   crates/core/src/metadata.rs:90`) is itself decoded from the same untrusted
   JSON as `inode.size` — a single malicious `ChunkRef { len: u64::MAX, .. }`
   entry reproduces the identical overflow/OOM bug one level down, so this
   doesn't remove the cause, it just relocates it to a field with the same
   trust class. The chosen fix instead grows the buffer off bytes `read_chunk`
   has *actually returned* (post checksum-verification), which is the only
   value in the whole call graph that is genuinely attacker-independent by the
   time it is used for a `Vec` operation.

3. **`Vec::try_reserve` (fallible allocation) instead of dropping the hint.**
   Considered to preserve *some* upfront reservation while turning an overflow
   into a `Result::Err` instead of a panic. Rejected: this still attempts a
   size-proportional allocation before the chunk map is validated (an attacker
   picking a size that fits `isize::MAX` but is still, say, 100 GB would cause
   a real 100 GB allocation attempt/OOM even though `try_reserve` wouldn't
   *panic*) — it satisfies "no panic" but not the brief's other binding
   condition, "no size-proportional allocation." The one-line `Vec::new()` fix
   satisfies both without adding a new fallible-alloc code path.

## Scope discipline

Confirmed the change touches only the allocation line and its comment,
`read_object_collecting` at `read.rs:74-98`. It does not touch:
- `crates/core/src/read.rs`'s `read_chunk` / EC-scheme dispatch (`read.rs:120-260`)
  — that's issue #285's territory per the brief's `Conflicts with: 285` /
  `Ordering note`.
- the metadata codec / on-disk format (ADR-0002, human-only per scope).
- no new size-limit configuration was introduced (see alternative 1 above).

## Test

Added `crates/core/src/read.rs` `#[cfg(test)] mod tests` (co-located, per the
brief's `Test file: crates/core/src/read.rs (tests module)`):
`oversized_inode_size_with_empty_chunk_map_errors_cleanly_not_panics`.

Builds a `Committed` `InodeRecord { size: u64::MAX, chunk_map: vec![], .. }`
(exactly the brief's repro) and calls `read_object_from` — the same production
entry point `read_object` / `read_path` funnel through
(`read.rs:57-67`) — against a `PlacementChunkStore` whose every method
`unreachable!()`s, so the test also proves the empty chunk map never reaches a
fragment fetch (it fails on the size check, not by accident succeeding for an
unrelated reason). Asserts `Err(..)` (not `Ok`, and — because the test itself
would abort the process — not a panic either) and sanity-checks the error
message mentions the actual/expected sizes involved, without hard-pinning to
the exact `ReadError::SizeMismatch` variant name (brief: "the specific error
variant is ILLUSTRATIVE; the BINDING condition is 'no panic / no
size-proportional allocation, and a clean error is returned'").

### Red→green proof (manual, via the project's runner — `cargo test`, the same
tool `engine/scripts/run-verify.sh` and `xtask ci` drive)

Ran in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`, checked out at target
`main` `b91401a`):

- **Post-fix (green):**
  `cargo test -p wyrd-core --lib read::tests` →
  `test read::tests::oversized_inode_size_with_empty_chunk_map_errors_cleanly_not_panics ... ok`
  (1 passed).
- **Pre-fix (red):** temporarily reverted only the allocation line back to
  `Vec::with_capacity(inode.size as usize)` (test kept as-is), reran the same
  command:
  ```
  thread '...' panicked at .../alloc/src/raw_vec/mod.rs:28:5:
  capacity overflow
  test read::tests::oversized_inode_size_with_empty_chunk_map_errors_cleanly_not_panics ... FAILED
  ```
  Then restored the fix (verified green again, shown above) before producing
  `patch.diff`.
- Full crate suite (`cargo test -p wyrd-core`, all 19 unit + 27 integration
  tests across `mutation_regressions*.rs`, `placement_record.rs`,
  `read_repair.rs`, `write_fanout.rs`) stays green with the fix — no
  regressions.

Also ran the bundle through the project's own gate script,
`engine/scripts/run-verify.sh` (`PDCA_BUNDLE=results/issue_290
WYRD_REPO=/home/eddie/wyrd/wyrd`): it applies `patch.diff` to a clean
`origin/main` worktree and runs `cargo test -p wyrd-core`. Because the test is
**co-located** inside the modified production file (`read.rs`) rather than a
separate `crates/*/tests/*.rs` file, the script classifies it as
"co-located" and — per its own documented behavior
(`engine/scripts/run-verify.sh:14-19,162-167`) — runs **green-only** (it
can't isolate a per-fix RED by reverting just the production hunk while
`patch.diff` also adds the test in the same file/hunk-group). Result:
`PASS (green-only) ... C4-ci gates the whole tree.` This is expected and
documented behavior of the gate for a co-located test, which is exactly what
the brief's `Test file: crates/core/src/read.rs (tests module)` specifies. The
manual revert-and-rerun above (using the same underlying `cargo test`
invocation the gate itself uses) supplies the actual red→green proof that
`run-verify.sh`'s co-located mode structurally can't isolate on its own.

## Formatting / commit-readiness

`cargo fmt --all -- --check` (the exact command `xtask ci` / the repo's commit
gate runs, `xtask/src/main.rs:533`) — clean, no diff.
`cargo clippy --workspace --exclude wyrd-dst --all-targets` scoped check on
`wyrd-core` — clean, no warnings (workspace lints treat warnings as errors per
`xtask/src/main.rs:534-542`).
