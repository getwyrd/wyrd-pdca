# Build notes — issue 287 / gc-identity-placement-fallback (iteration 2)

## Root cause (two sentences)

`referenced_fragments` (`gc.rs:189`) iterates `chunk.placement.iter().enumerate()`,
so a pre-M3 `ChunkRef` with `placement: vec![]` (decoded via `#[serde(default)]`,
`metadata.rs:93`) contributes zero entries to the reference set. A stale orphan record
whose grace window has expired then causes GC to call `delete_fragment` on the fragment
that the read path (`read.rs:fragment_dserver`, `read.rs:99-105`) would resolve to
D-server `index` via identity fallback — silent data loss on a live committed object.

## What was changed and why

The iteration-1 attempt was rejected on two grounds: (1) test breadth — only `None +
vec![]` at index 0 was covered; (2) the fix inlined a 3rd copy of the
expansion/identity-fallback logic alongside `read.rs:99-105` and
`reconstruction.rs:227-235`, raising a drift risk for future callers.

This iteration addresses both.

---

### 1. New shared helper — `impl ChunkRef` in `crates/core/src/metadata.rs`

Added two `pub` methods to `ChunkRef` (`metadata.rs:97-125`):

```rust
pub fn fragment_count(&self) -> u16
pub fn placed_dserver(&self, index: u16) -> DServerId
```

`placed_dserver` is the **single authoritative identity-fallback resolution** definition:

```rust
self.placement.get(index as usize).copied().unwrap_or(u64::from(index))
```

This is exactly what `read.rs:fragment_dserver` (`read.rs:99-105`) did inline, and what
`reconstruction.rs:227-235` did inline — now both delegate here instead of carrying
independent copies. `fragment_count` derives the total fragment count from the scheme
(`EcScheme::None` → 1; `ReedSolomon{k,m}` → `k+m`), which is the expansion dimension GC
previously lacked entirely.

Why on `ChunkRef` and not a free function or a separate module? `ChunkRef` already owns
both `scheme` and `placement` — the two inputs — so a method is the natural home. The
`impl` block is directly after the struct definition, visible to every caller without a
separate import.

### 2. `crates/core/src/read.rs` — delegate `fragment_dserver` to the shared helper

`read.rs:99-105` was the original inline implementation. After the change:

```rust
fn fragment_dserver(chunk: &ChunkRef, index: u16) -> DServerId {
    chunk.placed_dserver(index)
}
```

No behavioural change — the function is just a thin wrapper that now calls the
canonical definition. The wrapper is kept (rather than inlining `placed_dserver` at
call sites) to preserve the existing comment and the read path's narrative flow.

### 3. `crates/custodian/src/reconstruction.rs` — delegate placement expansion

`reconstruction.rs:226-235` previously had:

```rust
let n = k + m;
let placement: Vec<DServerId> = (0..n)
    .map(|i| chunk_ref.placement.get(i).copied().unwrap_or(i as DServerId))
    .collect();
```

After the change:

```rust
let placement: Vec<DServerId> = (0..chunk_ref.fragment_count())
    .map(|i| chunk_ref.placed_dserver(i))
    .collect();
```

`let n = k + m;` is removed because `n` was only used in `(0..n)`, and
`chunk_ref.fragment_count()` returns the same value for an RS scheme. `k` and `m` remain
in scope (still used in `survivors.len() < k` and in `RepairPlan { k, m, .. }`). No
type casts needed: `fragment_count()` returns `u16`, the range produces `u16`, and
`placed_dserver(i: u16)` accepts it.

**Cost of NOT centralising**: keeping the 3rd copy in gc.rs alongside read.rs and
reconstruction.rs means that a future change to placement semantics (e.g., a re-balance
or version-fence interaction) must be applied in 3 places by whoever knows about all
three. The centralised definition removes that obligation for all future authors.

### 4. `crates/custodian/src/gc.rs` — use the shared helpers in `referenced_fragments`

The broken loop at `gc.rs:189`:

```rust
for (index, dserver) in chunk.placement.iter().enumerate() {
    set.insert((*dserver, FragmentId { chunk: chunk.id, index: index as u16 }));
}
```

is replaced with:

```rust
for index in 0..chunk.fragment_count() {
    set.insert((chunk.placed_dserver(index), FragmentId { chunk: chunk.id, index }));
}
```

No import changes needed: `ChunkRef` methods are resolved from the inferred type
(`record.chunk_map: Vec<ChunkRef>`) without explicitly importing `ChunkRef` by name.

### 5. `crates/custodian/tests/gc.rs` — three regression sub-cases

Three tests added after `never_reclaims_a_referenced_fragment` (criterion 2), all under
the criterion-4 header "identity-fallback placement protects committed fragments":

| Test name | Scheme | placement | Orphan dserver/index | Flippable |
|---|---|---|---|---|
| `identity_fallback_none_empty_placement_protects_index0` | None | `vec![]` | (0, 0) | revert gc.rs loop |
| `identity_fallback_rs_empty_placement_protects_index_above_zero` | RS{2,1} | `vec![]` | (1, 1) | revert gc.rs loop |
| `short_placement_vector_fallback_protects_fallback_index` | RS{2,1} | `vec![5]` | (2, 2) | revert gc.rs loop |

Sub-case 4a covers `None + vec![]` (the brief's primary repro). 4b covers the
scheme-aware fragment-count expansion (`k+m = 3`) and a non-zero orphan index.
4c covers the mixed-explicit/fallback vector case (some indices explicit, others
identity-resolved).

## Red → green proof

Pre-fix (gc.rs reverted to `placement.iter().enumerate()`, metadata.rs and tests
unchanged):

```
running 6 tests
test identity_fallback_none_empty_placement_protects_index0 ... FAILED
test identity_fallback_rs_empty_placement_protects_index_above_zero ... FAILED
test short_placement_vector_fallback_protects_fallback_index ... FAILED
test result: FAILED. 3 passed; 3 failed
```

Post-fix (all changes applied):

```
running 6 tests
test honours_the_reader_safe_grace_window ... ok
test identity_fallback_none_empty_placement_protects_index0 ... ok
test identity_fallback_rs_empty_placement_protects_index_above_zero ... ok
test never_reclaims_a_referenced_fragment ... ok
test reclaims_expired_lease_byte_and_orphan_through_reconcile_step ... ok
test short_placement_vector_fallback_protects_fallback_index ... ok
test result: ok. 6 passed; 0 failed
```

Full `cargo xtask ci` exits 0 (fmt, clippy -D warnings, build, test, cargo-deny,
conformance vectors, DST).

## Citations (path:line on target branch, main)

| Brief citation | What I verified |
|---|---|
| `gc.rs:189` | `chunk.placement.iter().enumerate()` — the broken loop; replaced with `0..chunk.fragment_count()` + `chunk.placed_dserver(index)` |
| `read.rs:99-105` | `fragment_dserver` — the original inline fallback; now delegates to `ChunkRef::placed_dserver` |
| `metadata.rs:93` | `#[serde(default)]` on `placement: Vec<DServerId>` — why pre-M3 records decode with `vec![]` |
| `reconstruction.rs:227-235` | the second inline copy; now delegates to `(0..chunk_ref.fragment_count()).map(\|i\| chunk_ref.placed_dserver(i))` |
| `gc.rs:24,110` | GC durability invariant: "never reclaim a referenced fragment" — invariant restored by the fix |
</content>
</invoke>