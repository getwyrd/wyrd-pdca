# Build notes — issue 198 / read-path-chunk-id-recheck

Target: `getwyrd/wyrd @ main`. Worktree base = `origin/main` =
`037117705dcda12838fb3362b8aad823d65561f2` (the brief's repro cited `c2223a5`; the
read/repair code is unchanged between them — the cited line numbers still hold).

## Root cause (two sentences)

The read path admits a stored fragment on its self-describing checksum alone: both
fragment-decode sites in `read_chunk` call bare `decode(&fragment)` and accept on `Ok`
(`crates/core/src/read.rs:138-139` for `EcScheme::None`, `:176-177` for ReedSolomon),
omitting the `header.chunk_id == chunk` recheck that the shared verify
(`crates/core/src/repair.rs:53-55` `fragment_intact`, `:66-71` `intact_shard`) enforces
for scrub and reconstruction. A misplaced-but-intact fragment (valid checksum, foreign
`chunk_id` — a misrouted / placement-confused fragment) is therefore returned directly as
the `None`-scheme payload, or fed as a shard at `index` into the RS decoder → silent
corrupt reconstruction.

## The fix

Add the `decoded.header.chunk_id == chunk.id` guard to the accepting match arm at **both**
read sites in `read_chunk` (`crates/core/src/read.rs`):

- `EcScheme::None` site (was `:138-147`): `Ok(decoded) if decoded.header.chunk_id ==
  chunk.id => Ok(decoded.payload)`. A new `Ok(_)` arm handles the misplaced-but-intact
  case: it enqueues the chunk for repair (`corrupt.push`, the existing plumbing) and
  surfaces `ReadError::MissingFragment` — this chunk has no usable fragment present.
- ReedSolomon site (was `:176-192`): the accepting arm gains the same guard; the former
  `Err(_)` exclusion arm becomes `_` so a misplaced fragment is read around exactly as a
  checksum-failing one is (excluded from the decoder + enqueued).

The guard `decoded.header.chunk_id == chunk.id` is **literally the same predicate** the
shared verify uses (`repair.rs:54`, `:68`: `decoded.header.chunk_id == chunk`), satisfying
the brief's BINDING constraint that the check "is the same one scrub/reconstruction use."

### Why inline guard, not `repair::intact_shard` / `fragment_intact` (Do's call)

The brief marks routing-through-the-shared-fn vs. an inline check as ILLUSTRATIVE. I chose
the inline guard for three concrete reasons:

1. **It makes the existing doc accurate without editing out-of-scope code.** `repair.rs:50`
   already documents the intended architecture: "the read path **decodes for the same
   effect inline** (`crates/core/src/read.rs`)". An inline `chunk_id` guard makes that
   sentence true; routing through `intact_shard` would make it stale ("inline" → "calls
   this"), and the brief scopes edits to `read.rs` only ("edits only
   `crates/core/src/read.rs`"), so I cannot fix the comment in `repair.rs`. The inline
   form keeps both files consistent with one edit confined to the scoped file.
2. **It preserves the existing corrupt-fragment error contract.** `intact_shard` collapses
   "checksum fail" and "wrong chunk_id" into one `None`, which would change the `None`-
   scheme corrupt-fragment read from the precise `FragmentError` (e.g.
   `PayloadChecksumMismatch`) to a generic error — a behaviour change *outside* this
   brief's scope (the corrupt path is `#207` / not this fix). The three-arm inline match
   keeps `Err(e) => Err(e.into())` untouched for the genuinely-corrupt case and adds only
   the misplaced case.
3. **No new coupling, minimal diff.** `read.rs` already `use`s `crate::repair` and calls
   `repair::enqueue_repair`, so neither option adds a dependency. The inline guard is a
   one-token addition to the accepting arm at each site; the diff is +34/-7 in `read.rs`
   (mostly comments). Routing through `intact_shard` would be a comparable line count
   *plus* the doc-staleness in (1).

This is **cause removal at the one violating module**, per the brief's Invariant-to-restore
self-test: the read path was the sole consumer admitting on checksum alone; scrub already
uses `fragment_intact`, reconstruction already uses `intact_shard`. After this change all
three consumers enforce `decode-clean AND chunk_id-match`.

### Error choice for the `None` misplaced case

`ReadError::MissingFragment { chunk_id }` — the store returned a *foreign* fragment, so the
chunk's *own* fragment is effectively absent. Its message ("committed chunk map references
missing fragment …") reads correctly. I did not add a new `ReadError` variant: that would
be more than the minimal change, and the store-layer corruption-error contract is
explicitly out of scope (`#207`).

### Enqueue behaviour

The brief leaves "whether the read path *enqueues* a repair obligation for the misplaced
fragment" out of scope ("existing … plumbing retained as-is"). By routing the misplaced
case through the same `corrupt.push(chunk.id)` path the corrupt case already uses, the
misplaced fragment is enqueued for repair as a side effect — no new plumbing, the existing
behaviour retained. The tests deliberately do **not** assert on the enqueue (the brief
makes only non-admission binding), so they stay pinned to exactly the binding property.

## Tests — `crates/core/tests/read_repair.rs` (the file the brief names)

Two `#[tokio::test]`s appended after the existing leg-4 tests, reusing this file's in-memory
`MemMeta` / `MemChunks` trait stores and `fragment()` / `commit_inode()` helpers (no GUI /
heavy deps — runs headless under `cargo test`):

- `none_read_rejects_a_misplaced_but_intact_fragment` — stores at index 0 a valid fragment
  whose header names a *different* chunk, same payload length as the read chunk (so a
  pre-fix admit clears the inode size check and returns foreign bytes). Asserts the read is
  `Err`.
- `ec_read_treats_a_misplaced_but_intact_fragment_as_absent` — RS(2,1) with index 0 =
  genuine data shard, index 2 = missing, index 1 = a misplaced-but-intact fragment of the
  same shard length filled with `0xFF`. With only 1 genuine survivor (< k), post-fix the
  misplaced shard is excluded → `Err(InsufficientFragments)`. The 200-byte payload spans
  both data shards, so pre-fix the foreign shard corrupts the *live* output (bytes 128..199
  become `0xFF`) — a faithful "silent corrupt reconstruction", not padding truncation hides.

### Why deterministic (no any-k ordering flakiness)

The any-k-arrive-first read picks whichever `k` decode first, so any setup with **> k**
valid candidates is order-dependent (`FuturesUnordered` poll order). I avoided that by
making the misplaced fragment *required* to reach k: exactly k slots present, one genuine +
one misplaced, the rest missing. Post-fix the misplaced one is excluded → below k →
deterministic `Err`. (An earlier 46-byte RS payload made pre-fix reconstruction
*accidentally correct* because the second data shard was all zero-padding that truncation
discarded; enlarging to 200 bytes fixes that so the bug's harm is visible.)

## Red → green evidence

Run via `cargo test -p wyrd-core --test read_repair` (the same `cargo test` mechanism
`cargo xtask ci` invokes; Bash-tool timeout in lieu of the gate's):

- **With fix:** 4 passed (the 2 existing leg-4 tests + the 2 new).
- **Production reverted** (`git stash push -- crates/core/src/read.rs`, test kept): the 2
  new tests FAIL —
  - `none_…`: `got Ok(Some([97,110,111,…]))` = the foreign payload `"another chunk's
    bytes!"` returned.
  - `ec_…`: `got Ok(Some([0,1,…,127, 255,255,…]))` = correct first 128 bytes then `0xFF`
    garbage from the foreign shard — silent corrupt reconstruction.
  Restored → 4 passed again.

### C4-verify (`run-verify.sh`) note for sign-off

The brief names the **existing** file `crates/core/tests/read_repair.rs`, so the test lands
as a *modification*, not a newly-added `tests/*.rs`. `run-verify.sh` isolates the RED leg
only for newly-*added* test files; for a modification it degrades to **green-only** (exit 0)
and relies on `C4-ci` (`cargo xtask ci`) to gate the whole tree. The full red→green is
nonetheless real and is the manual evidence above (revert `read.rs` only, keep the test →
2 fail). This is a mechanical property of putting the test in the brief-named existing
file, not a weakness of the test.

## Gate-readiness

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-core --all-targets`: clean (workspace lints = deny warnings).
- `git apply --check patch.diff` on a clean `origin/main` tree: applies.

## Existing tests unaffected

`ec_read_excludes_corrupt_fragment_and_enqueues_for_repair` and
`unrecoverable_read_still_enqueues_the_corrupt_chunk` still pass: their genuine fragments
satisfy the new `chunk_id` guard, and the corrupt single-fragment path keeps its original
`Err(e) => Err(e.into())` arm.
