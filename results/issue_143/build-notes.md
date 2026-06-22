# Build notes — issue 143 / m3.5-scrub-custodian (iteration 2)

Target: getwyrd/wyrd @ main (worktree off `origin/main`, base `a157aba`).
Planning artifact read: `docs/design/proposals/accepted/0005-milestone-3-custodians.md`
(§Scrub `0005:262-267`; §6.3 step 1 / read-vs-scrub mirror `0005:264-266`; read-time-
failure feed `0005:174-176`; durability metrics `0005:331-332`; PR-sequence slice 5
`0005:528-530`; Option A `0005:524-527`).

## What iteration 1 left open (the carry-forward)

The v1 implementation was accepted on its mechanics; the single blocker was a **test
gap (T5)**. `repair::fragment_intact` (`crates/core/src/repair.rs:53-55`) guards **two**
conditions:

```rust
matches!(wyrd_chunk_format::decode(bytes), Ok(decoded) if decoded.header.chunk_id == chunk)
```

— (a) the bytes decode cleanly (header + payload crc32c verified by the on-disk-format
reader), AND (b) the decoded header names the **expected** chunk (the id the committed
chunk map references the fragment under). v1's test set only exercised **half (a)**: an
injected bit-flip in the payload (`crates/custodian/tests/scrub.rs` leg 2,
`crates/core/tests/read_repair.rs` leg 4). Half (b) — an **intact-but-misplaced**
fragment whose checksum verifies yet whose header names a *different* chunk — was
untested, so nothing pinned that guard as load-bearing.

This iteration **changes no production code**; it adds the missing regression and proves
half (b) is load-bearing. The v1 patch content is carried forward verbatim (re-applied to
the same base and re-verified), and the new test is appended.

## The fix: a misplacement regression in the scrub test

New test `detects_a_misplaced_intact_fragment_excludes_and_enqueues_for_reconstruction`
in `crates/custodian/tests/scrub.rs` (after leg 2). It stores a **valid** v1 fragment for
chunk `0x9999` at the storage location `(chunk 0xC8, index 0)` that the committed chunk
map references — `valid_fragment(foreign)` put at `frag(chunk, 0)`. The fragment's payload
checksum verifies cleanly, but its header names `0x9999`, not `0xC8`. Scrub fetches it,
calls `repair::fragment_intact(bytes, frag.chunk)` = `fragment_intact(.., 0xC8)`, which
decodes OK but `header.chunk_id (0x9999) != 0xC8` → `false` → the fragment is a corruption
finding. Asserts:

- `reconcile_step(..) == Changed` — it is **detected**, not silently absorbed.
- `queued_repairs == [0xC8]` — the **referenced** chunk is enqueued (excluded fragment),
  and `repair_key(0x9999)` is **absent** — the stray header's id is never the obligation.
- the fragment is **not deleted** (scrub only produces obligations; reclaim/rebuild are
  GC / slice 6).

### Why the scrub leg, not the read-path leg

The carry-forward allows "scrub leg ... and/or the read-path leg." The misplacement guard
is `fragment_intact`'s, and `fragment_intact` has **exactly one caller** —
`crates/custodian/src/scrub.rs:60` (`repair::fragment_intact(&bytes, frag.chunk)`). The
scrub test is therefore the *only* place that exercises the guard directly, so it is the
correct and sufficient home for the regression. (The read path, `crates/core/src/read.rs`,
verifies the checksum inline by decoding; the header-against-map cross-check is scrub's
proactive job — `0005:264-266` "verify each referenced fragment's self-describing checksum
**against the committed chunk map**". The stated invariant's subject is "a fragment whose
checksum **fails** verification"; a misplaced-but-intact fragment is an *additional* guard
`fragment_intact` adds beyond that subject, and it lives only on the scrub side.)

## Demonstrated red (half (b) is load-bearing, not absence)

Negated the chunk-id half of the guard in `crates/core/src/repair.rs:54`:

```rust
// pub fn fragment_intact(bytes: &[u8], chunk: ChunkId) -> bool {
{ let _ = chunk; wyrd_chunk_format::decode(bytes).is_ok() }   // (a) only
```

Ran `cargo test -p wyrd-custodian --test scrub`:

```
test detects_a_misplaced_intact_fragment_...  FAILED
  left: Satisfied, right: Changed   (the misplaced fragment was absorbed silently)
test result: FAILED. 3 passed; 1 failed
```

Only the new test fails — the three pre-existing scrub tests (walk+verify, bit-flip,
telemetry) still pass — proving the new test pins *precisely* the
`header.chunk_id == chunk` half and nothing else. Guard restored; all 4 green again.

## Verification (this iteration)

- `cargo test -p wyrd-custodian --test scrub` → 4 passed (incl. the new misplacement leg).
- `cargo test -p wyrd-core --test read_repair` → 2 passed (read-path leg 4 unchanged).
- `cargo test -p wyrd-custodian --test gc --test skeleton` → 4 + 3 passed (the
  `reconcile_step` signature change is unchanged from v1; no regression).
- `cargo fmt -p wyrd-custodian -p wyrd-core -- --check` → exit 0 (rustfmt re-wrapped one
  `meta.get(..).await.unwrap().is_none()` chain; applied before generating patch.diff).
- `cargo clippy -p wyrd-custodian --tests` → clean.
- `git apply --check patch.diff` on a clean base `a157aba` → applies clean.
- The whole gate `cargo xtask ci` (via `./engine/xtask.sh ci`) is Check's to re-run.

## Files in the bundle

- `patch.diff` — full slice (v1 content + the new scrub regression), against `main`.
- `scrub.rs` — the brief-named test file (`crates/custodian/tests/scrub.rs`), now 4 tests.
- `read_repair.rs` — the read-path leg-4 regression (`crates/core/tests/read_repair.rs`),
  unchanged from v1, shipped here because the enqueue seam lands in `core`.

## Unchanged from v1 (recap, since the design is carried forward)

- Shared queue lives in `core` (`crates/core/src/repair.rs`) because `custodian → core`
  is the only legal dependency direction (ADR-0010, `0005:421-422`); both producers call
  the same `enqueue_repair` against the same `repair_key`, so "one shared queue" holds by
  construction.
- Scrub dispatches through the real `reconcile_step` fenced control point
  (`crates/custodian/src/reconciliation.rs`), added as a `scrub: Option<&ScrubContext>`
  parameter; `None,None` returns `Satisfied` exactly as before (gc/skeleton unchanged).
- Read path feeds the same queue without changing `read_object_from`'s public signature
  (~25 callers carry no `MetadataStore`); the enqueue runs in `read_object`, which already
  has `meta` in scope.
- Cargo.lock: one line — `wyrd-chunk-format` added to `wyrd-custodian`'s **dev**-deps
  (workspace-internal; no new external dependency, ADR-0003). No on-disk-format change.
- Out of scope and untouched: the reconstruction custodian (dequeue/rebuild/re-place/
  version-conditional commit) and repair-vs-serve priority — slice 6 (`0005:531-536`).
