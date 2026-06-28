# Build notes — issue 287 / gc-identity-placement-fallback

## Root cause (two sentences)

`referenced_fragments` (`gc.rs:189`) iterates `chunk.placement.iter().enumerate()`, so a
pre-M3 `ChunkRef` with `placement: vec![]` (decoded via `#[serde(default)]`,
`metadata.rs:93`) contributes zero entries to the reference set. A stale orphan record
whose grace window has expired then causes GC to call `delete_fragment` on the fragment
that the read path (`read.rs:fragment_dserver`, `read.rs:99-105`) would resolve to
D-server 0 via identity fallback — silent data loss on a live committed object.

## What was changed and why

### `crates/custodian/src/gc.rs`

**Line 32** — added `EcScheme` to the `wyrd_core::metadata` import so the scheme match
works without `metadata::EcScheme::…` verbose paths.

**Lines 188–214** (post-edit numbering) — replaced the single-iterator loop:

```rust
// Before (gc.rs:189 on main):
for (index, dserver) in chunk.placement.iter().enumerate() {
    set.insert((*dserver, FragmentId { chunk: chunk.id, index: index as u16 }));
}
```

with a scheme-aware, identity-fallback expansion:

```rust
// After:
let n: u16 = match chunk.scheme {
    EcScheme::None => 1,
    EcScheme::ReedSolomon { k, m } => u16::from(k) + u16::from(m),
};
for index in 0..n {
    let dserver = chunk.placement.get(index as usize).copied().unwrap_or(u64::from(index));
    set.insert((dserver, FragmentId { chunk: chunk.id, index }));
}
```

This mirrors exactly:
- `read.rs:fragment_dserver` (`read.rs:99-105`): `placement.get(index).copied().unwrap_or(u64::from(index))`
- `reconstruction.rs:227-235`: the same `(0..n).map(|i| chunk_ref.placement.get(i).copied().unwrap_or(i as DServerId))` expansion

The fix is also inherited by `scrub.rs:58` which calls `gc::referenced_fragments` directly
(noted in `scrub.rs:32`: `use crate::gc::referenced_fragments;`), so the scrub gap
described in the brief is closed by the same change.

## Why not a dedicated placement-expansion helper (centralise approach)?

The brief states "Centralizing the ChunkRef placement expansion so read/scrub/GC/reconstruction share one definition is acceptable and preferred, but the binding requirement is GC's reference set, not a refactor."

Cost of the centralised approach (diff sketch):
- New `pub fn expand_placement(chunk: &ChunkRef) -> impl Iterator<Item=(u16, DServerId)>` (or similar) added to `crates/core/src/metadata.rs` or a new `crates/core/src/placement_expansion.rs` — ~10 lines of new public API
- Touch `read.rs:fragment_dserver` + `reconstruction.rs:227-235` to call the helper — 2 call-sites modified
- Touch `gc.rs` to call the helper — the 1 call-site this fix needs anyway
- Roughly **3 files modified vs 1** for the same behavioural outcome

The inline approach is identical in correctness and smaller in scope: the two pre-existing call-sites (`read.rs`, `reconstruction.rs`) already have the correct idiom inlined; adding a public helper abstraction is a refactor that 288 (which edits `read.rs`) might need to rebase on. The brief explicitly permits inline-only and notes "schedule [287 and 288] into different waves so neither builds blind on the other's base."

The inline approach chosen here is **not a guard-the-symptom** short-cut: it restores the invariant directly — GC's reference set now equals the read path's resolved placement closure, which is the stated invariant. The centralised path is a code-quality improvement for a future refactor cycle, not a correctness prerequisite.

## Red→green verification

**Pre-fix (conceptual):** With the original `chunk.placement.iter().enumerate()` loop and
`placement: vec![]`, the loop yields nothing. The orphan record for
`(dserver=0, FragmentId { chunk: 0xF600, index: 0 })` survives the reference check (set
is empty), GC calls `delete_fragment`, `d0.get_fragment(frag(0xF600, 0))` returns `None`,
and the test assertion `assert!(... .is_some())` fires → RED.

**Post-fix:** `referenced_fragments` expands to `n=1` (EcScheme::None), resolves index 0
→ `placement.get(0).unwrap_or(0)` = 0 → inserts `(0, FragmentId { chunk: 0xF600,
index: 0 })` into the set. GC's SAFETY GATE hits `continue`; fragment is not reclaimed.
Test assertions hold → GREEN.

Confirmed by running `./engine/xtask.sh ci` (the project's gating runner, `pdca.toml` C4-ci):
- `tests/gc.rs`: 4 passed (3 existing + 1 new), 0 failed
- Full suite: all checks passed

## Citations (path:line on target branch, main)

| Citation in brief | What I read |
|---|---|
| `gc.rs:189` | the `chunk.placement.iter().enumerate()` loop — the broken line |
| `read.rs:99-105` | `fragment_dserver`: the identity-fallback the fix mirrors |
| `metadata.rs:93` | `#[serde(default)]` on `placement: Vec<DServerId>` |
| `reconstruction.rs:227-235` | the `(0..n).map(|i| .get(i).copied().unwrap_or(i as DServerId))` expansion |
| `gc.rs:24,110` | the GC durability invariant: "never reclaim a referenced fragment" |
