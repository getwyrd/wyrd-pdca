# Build notes — issue 203 / fschunkstore-unique-temp-per-write

Target: `getwyrd/wyrd @ main` (worktree base `0371177`). All citations are
`path:line` in that worktree after the patch.

## Root cause (two sentences)

`FsChunkStore::put_fragment` wrote every fragment through a scratch path keyed on
the `FragmentId` **alone** — `<chunk>/<index>.tmp` (old `temp_path`, original
`lib.rs:44-49`) — then `fs::write(&temp, …)` + `fs::rename(&temp, final)`. Two
concurrent writes of the *same* id therefore shared one scratch file, so the
second `fs::rename` could find the temp already moved by the first and fail
`NotFound` (ENOENT), spuriously erroring a legitimate idempotent-retry / repair
write.

## Invariant restored (the brief's load-bearing field)

> Two concurrent writes of the same `FragmentId` must each use **private scratch**
> and publish via an **atomic rename**.

The fix makes the scratch path unique **per call** and leaves the atomic rename as
the *sole* publish/serialization point. Last-writer-wins is then a semantic no-op
(same id ⇒ identical checksum-verified EC bytes), and writes of *different*
fragments stay independent (they always had distinct final paths). This is the
write idiom the store already intended ("write-then-rename so a crash never leaves
a half-written fragment visible", `lib.rs:42-43` originally) — it only broke by
sharing the scratch path. Source for atomicity: POSIX `rename(2)` same-filesystem
atomic-replace (Tier A platform canon).

## What changed

- **Unique per-write scratch** — `temp_path` now appends this process's pid and a
  process-global `AtomicU64` sequence: `<index>.<pid>.<seq>.tmp`
  (`temp_path` at `crates/chunkstore-fs/src/lib.rs:96-104`; counter
  `TEMP_SEQ` at `:27`; import at `:17`). The atomic counter guarantees
  distinct names for concurrent in-process writers (the exact case the d-server
  hits over `Arc<store>` / `from_arc`); the pid distinguishes processes. No two
  concurrent calls ever name the same scratch file, so none can observe or
  clobber another's partial bytes, and each renames only its *own* temp — the
  ENOENT race is structurally gone.
- **Atomic publish + own-scratch cleanup** — `put_fragment` writes the private
  scratch then `fs::rename`s it onto the final path; on a failed write *or* rename
  it removes **its own** uniquely-named scratch (never a concurrent write's)
  (`:143-152`). This replaces the old scheme's accidental self-cleaning (the next
  same-id write overwrote the single fixed `.tmp`).
- **Crash-litter reaping** — unique names mean a *hard* crash (process killed /
  power loss, where the in-process cleanup above never runs) leaves an orphan
  scratch that nothing later overwrites. `open` now reaps it: `reap_stale_temps`
  walks recognised `<32-hex>` chunk dirs and removes any `*.tmp`
  (`open` at `:37-49`, `reap_stale_temps` at `:51-81`, suffix match
  `is_temp_scratch_name` at `:295-298`). **Open is the chosen reap site
  deliberately**: a `D server` owns its root single-process, and at open no write
  on *this* store is in flight, so reaping can never race a live put's scratch —
  unlike reaping inside `put_fragment`, which would race a concurrent same-chunk
  writer's in-flight temp. Reaping is best-effort (an unreadable/unremovable entry
  is left in place; scratch is harmless — `list_fragments` parses only `.frag`).

`list_fragments` / `parse_fragment_file_name` were left untouched: they already
accept only names ending exactly `.frag` (`:233-235` region), so every `.tmp`
shape — old `<index>.tmp` and new `<index>.<pid>.<seq>.tmp` — stays invisible. The
brief's "list_fragments continues to ignore temp scratch" criterion holds for free
and is asserted by the test and the unchanged `list_skips_foreign_and_temp_entries`
conformance case.

## Why this shape, and alternatives ruled out

- **`tempfile` crate (`NamedTempFile` + `.persist`)** — would also give private
  scratch + atomic rename and self-cleans on drop. Rejected because `tempfile` is
  currently only a **dev-dependency** of this crate (`Cargo.toml [dev-dependencies]`).
  Promoting it to a runtime `[dependencies]` adds a **new production dependency**,
  which per `docs/INTEGRATION.md §4` is a maintainer/human-only item (ADR-0003
  three-test audit + `deny.toml` allowlist) — it would inject a NEEDS-HUMAN flag
  for zero behavioural gain over ~3 lines of `std`. The atomic-counter+pid scheme
  the brief lists first needs **no** new dependency.
- **Per-fragment mutex map** — explicitly out of scope in the brief, and the
  Invariant self-test rules it out: it is in-process-only (no cross-process /
  crash-safety the rename already provides) and adds machinery. The structural fix
  (object/scratch-file lifetime, `principles.md §1.2`) is smaller and is what the
  invariant actually demands.
- **Guarding a single call site** — the invariant is over *concurrent same-id
  writes*, not one path, so a guard on one call is not a fix (brief self-test). The
  change is at the scratch-naming layer, which is where every writer is.

`fsync` durability of the temp/parent dir is **out of scope** (the brief files it
separately); the patch keeps the existing non-fsync `std::fs` write, so an
acknowledged fragment can still be lost on power loss — unchanged from `main`.

## Red→green proof (deterministic green, robust red)

Test: `crates/chunkstore-fs/tests/concurrent_put.rs` (net-new). 64 writers released
together by a `Barrier`, repeated over 16 rounds, all writing one fixed
`FragmentId` with identical valid bytes; asserts every put is `Ok`, the published
fragment round-trips/verifies, and `list_fragments` returns exactly `[id]`. Sync
`std::fs` I/O is driven by real OS threads via `pollster::block_on` — genuine
concurrency, **no async runtime / no GUI / headless-safe** (import-light: bytes,
pollster, tempfile, the two wyrd crates).

- **Red (fix reverted, test kept):** FAILS on **round 0, writer 0** —
  `No such file or directory (os error 2)` (the ENOENT rename race). The red is
  robust, not intermittent: it tripped on the first burst.
- **Green (fix applied):** passes; full crate suite green — 3 unit + 1 new + 9
  conformance, no regressions.

Runner note: `cargo xtask` exposes only whole-suite subcommands (`ci`, …), no
single-test mode, so the red→green flip used a targeted `cargo test -p
wyrd-chunkstore-fs --test concurrent_put` **bounded by the tool timeout** (no hang
risk). The authoritative gate `./engine/xtask.sh ci` (fmt --check, clippy -D
warnings, build, test, deny, conformance) is Check's to run; `cargo fmt -p
wyrd-chunkstore-fs -- --check` and `cargo clippy -p wyrd-chunkstore-fs --all-targets
-- -D warnings` both pass here, so the patch is commit-ready for the target's hooks.

## Citations

- `crates/chunkstore-fs/src/lib.rs:17` — `use std::sync::atomic::{AtomicU64, Ordering};`
- `crates/chunkstore-fs/src/lib.rs:27` — `static TEMP_SEQ`
- `crates/chunkstore-fs/src/lib.rs:37-49` — `open` reaps at startup
- `crates/chunkstore-fs/src/lib.rs:51-81` — `reap_stale_temps`
- `crates/chunkstore-fs/src/lib.rs:96-104` — unique `temp_path`
- `crates/chunkstore-fs/src/lib.rs:143-152` — write→rename + own-scratch cleanup
- `crates/chunkstore-fs/src/lib.rs:295-298` — `is_temp_scratch_name`
- `crates/chunkstore-fs/tests/concurrent_put.rs` — the regression test
