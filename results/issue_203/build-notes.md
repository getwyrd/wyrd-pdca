# Build notes — issue 203 / fschunkstore-unique-temp-per-write (iteration 2)

Target: `getwyrd/wyrd @ main` (worktree base `0371177`). Citations are `path:line`
in that worktree after the patch.

## TL;DR for Check — the iteration-1 §6.1 rejection was a misattribution

Iteration 1 was rejected because `cargo xtask ci` went red on the madsim DST
tier (exit 101), and the sign-off attributed that to the patch (main was seen
green 3/3). **I root-caused it: the DST red is a pre-existing flaky test on clean
`main`, unrelated to this change.** Decisive evidence below. The same-id race fix
itself was sound; this iteration keeps the fix, makes the scratch logic strictly
*more* simulation-isolated (removes the process-global state the carry-forward
flagged as a "prime suspect"), and adds a structural uniqueness assertion.

## Root cause of the issue (two sentences)

`FsChunkStore::put_fragment` wrote every fragment through a scratch path keyed on
the `FragmentId` **alone** — `<chunk>/<index>.tmp` (original `lib.rs:44-49`) —
then `fs::write(&temp, …)` + `fs::rename(&temp, final)`. Two concurrent writes of
the *same* id therefore shared one scratch file, so the second `fs::rename` could
find the temp already moved by the first and fail `NotFound` (ENOENT), spuriously
erroring a legitimate idempotent-retry / repair write.

## Root cause of the iteration-1 DST failure (the carry-forward task)

The carry-forward asked: "determine why the patched FsChunkStore flips the madsim
DST tier red," naming three suspects (the reap `fs::read_dir` at every `open`,
`std::process::id()`, the process-global `AtomicU64`). I reproduced and falsified
all three:

1. **madsim seeds default to wall-clock time.** madsim's builder sets the seed to
   "seconds since the Unix epoch" when `MADSIM_TEST_SEED` is unset
   (`madsim-0.2.34/src/sim/runtime/builder.rs:30`, `:65`), and `MADSIM_TEST_NUM=50`
   sweeps `seed..seed+50`. So `cargo xtask dst` tests a *different* 50-seed window
   every run — the failure is seed-dependent, not deterministic per invocation.
   This is why iteration 1 saw it once and the sign-off's "main green 3/3" missed
   it: 3 runs ≈ 150 seeds, and the flake's hit rate is low enough to slip through.

2. **The failing test is `durability_emission_rises_then_returns_to_zero` in
   `crates/dst/tests/custodian.rs` — not a FsChunkStore test.** It builds its
   stores with `MemMeta::default()` and in-memory `servers()`/`fleet_of`
   (`custodian.rs:928-931`); it never constructs an `FsChunkStore`. Its assertion
   that flakes is on tracing-metric capture across `reconcile_step`
   (`custodian.rs:956-970`), i.e. a `tracing` subscriber-ordering nondeterminism,
   structurally disjoint from chunk-store scratch paths.

3. **It reproduces on clean `main` with the patch reverted.** With the iteration-1
   patch *and* on bare `main`, sweeping the fixed window `MADSIM_TEST_SEED=201
   MADSIM_TEST_NUM=200` flips `durability_emission_…` red intermittently
   (observed ~2/4 rounds), passing other rounds with the *same* seed window — a
   genuine flake, not a patch regression. The FsChunkStore-exercising DST tests
   (`concurrency.rs`, `network.rs`) pass deterministically across that same window
   with this patch, and `MADSIM_TEST_CHECK_DETERMINISM=1` reports **no**
   nondeterminism for the patched code.

**Conclusion:** none of the three suspects is the cause; the DST red is a
pre-existing flaky custodian test. It is a separate defect (a tracing-capture
nondeterminism in `wyrd-dst`), to be **filed separately** — exactly as the brief
already files `fsync` durability separately. It is out of scope for issue 203
(different crate, different root cause) and fixing it here would violate
one-logical-change-per-PR. **Check should not re-attribute a `durability_emission`
DST flake to this patch; reproduce on `main` to confirm.**

Reproduction recipe (either tree):
`cd $PDCA_WORKTREE && RUSTFLAGS="--cfg madsim" MADSIM_TEST_SEED=201 MADSIM_TEST_NUM=200 cargo test -p wyrd-dst --test custodian`
(re-run a few times; `durability_emission_rises_then_returns_to_zero` flips red
intermittently regardless of this patch).

## Invariant restored (the brief's load-bearing field)

> Two concurrent writes of the same `FragmentId` must each use **private scratch**
> and publish via an **atomic rename**.

The fix makes the scratch path unique **per call** and leaves the atomic rename as
the *sole* publish/serialization point. Last-writer-wins is then a semantic no-op
(same id ⇒ identical checksum-verified EC bytes), and writes of *different*
fragments stay independent (distinct final paths, distinct scratch). This is the
write idiom the store already intended ("write-then-rename so a crash never leaves
a half-written fragment visible") — it only broke by sharing the scratch path.
Source: POSIX `rename(2)` same-filesystem atomic-replace (Tier A platform canon).

## What changed (this iteration), and how it differs from iteration 1

- **Per-store scratch sequence, not a process-global static.** The uniqueness
  counter is now an instance field `scratch_seq: AtomicU64` on `FsChunkStore`
  (`lib.rs:36`, init `lib.rs:47`, import `lib.rs:17`), consumed in `temp_path`
  (`lib.rs:73-78`). Iteration 1 used a *process-global* `static TEMP_SEQ` plus
  `std::process::id()` — the exact shared-global-mutable-state and
  process-introspection the carry-forward flagged. The store (one
  `Arc<FsChunkStore>` shared across gateway/custodian writers via `from_arc`) is
  the concurrency boundary every racing same-id write passes through, and
  **ADR-0034 §Decision (`docs/design/adr/0034-d-server-disk-model.md:52`) adopts
  Model A — one D server per disk**, so one process owns each root. A per-store
  counter therefore gives every concurrent writer a private scratch path with *no*
  shared global state and *no* `process::id()` — strictly better simulation
  isolation (independent madsim nodes can't be coupled through a global), and it
  drops two of the three named suspects entirely. Scratch name is now
  `<index>.<seq>.tmp` (`scratch_file_name`, `lib.rs:306-308`).
- **§6.2 — reap scoping confirmed and documented.** `reap_stale_temps`
  (`lib.rs:88-111`) runs at `open` only. Its one-opener-per-root assumption is now
  *justified by citation*, not assumed: ADR-0034 Model A means one D server owns
  the root, and at `open` no write on the just-constructed store is in flight, so
  reaping can never delete a concurrent put's in-flight scratch. It matches `.tmp`
  by suffix (`is_temp_scratch_name`, `lib.rs:314-316`) deliberately — that reaps
  both the new `<index>.<seq>.tmp` and any legacy `<index>.tmp`, and never a
  `.frag` (which is the only thing `list_fragments` accepts).
- **§6.3 — structural uniqueness assertion added.** A unit test asserts per-write
  privacy *structurally*, independent of interleaving: distinct `seq` ⇒ distinct
  scratch name, the scratch name is never parsed as a fragment, and it is
  recognised as reapable (`scratch_names_are_unique_per_seq_and_invisible_to_listing`,
  `lib.rs:369`). This complements (does not replace) the behavioural concurrency
  stress test.
- **Atomic publish + own-scratch cleanup** — `put_fragment` writes the private
  scratch then `fs::rename`s onto the final path; on a failed write *or* rename it
  removes **its own** uniquely-named scratch (never a concurrent write's)
  (`lib.rs:150-159`).

`list_fragments` / `parse_fragment_file_name` are untouched: they accept only
names ending exactly `.frag`, so every `.tmp` shape stays invisible. The brief's
"list_fragments continues to ignore temp scratch" criterion holds for free and is
asserted by the concurrency test and the unchanged conformance case
`list_skips_foreign_and_temp_entries`.

## Why this shape, and alternatives ruled out (with costs)

- **`tempfile` crate (`NamedTempFile` + `.persist`)** — would also give private
  scratch + atomic rename. Rejected because `tempfile` is only a
  **dev-dependency** here (`Cargo.toml:17-19`); promoting it to a runtime
  `[dependencies]` adds a **new production dependency**, which per
  `docs/INTEGRATION.md §4` is a maintainer/human-only item (ADR-0003 three-test
  audit + `deny.toml` allowlist). Concretely that is a NEEDS-HUMAN flag + a
  `Cargo.toml` `[dependencies]` line + `deny.toml` review, for **zero** behavioural
  gain over the `scratch_file_name` one-liner (`lib.rs:306-308`) + a 1-field
  struct change. The per-store counter needs no new dependency.
- **Per-fragment mutex map** — explicitly out of scope in the brief, and the
  Invariant self-test rules it out: in-process-only (no crash-safety the rename
  already gives) and adds a `Mutex<HashMap<FragmentId, …>>` + lock-lifetime
  machinery. The structural fix (object/scratch-file lifetime, `principles.md
  §1.2`) is smaller and is what the invariant demands.
- **Keeping the process-global static + pid (iteration 1)** — works functionally,
  but is the shared-global-mutable-state madsim isolation anti-pattern and was a
  named rejection suspect. The per-store counter removes it at no cost (same line
  count), so re-submitting it unchanged is both disallowed and worse.

`fsync` durability of the temp/parent dir is **out of scope** (the brief files it
separately); the patch keeps the existing non-fsync `std::fs` write, unchanged
from `main`.

## Red→green proof (deterministic green, robust red)

Test: `crates/chunkstore-fs/tests/concurrent_put.rs` (net-new, the brief-named
file). 64 writers released together by a `Barrier`, repeated over 16 rounds, all
writing one fixed `FragmentId` with identical valid bytes; asserts every put is
`Ok`, the published fragment round-trips/verifies, and `list_fragments` returns
exactly `[id]`. Synchronous `std::fs` I/O driven by real OS threads via
`pollster::block_on` — genuine concurrency, **no async runtime / no GUI /
headless-safe** (import-light: bytes, pollster, tempfile, the two wyrd crates).

- **Red (scratch reverted to the shared `<index>.tmp`, test kept):** FAILS on
  **round 0, writer 0** — `No such file or directory (os error 2)` (the ENOENT
  rename race). Robust, not intermittent: it tripped on the first burst.
- **Green (fix applied):** passes; full crate suite green — 4 unit (incl. the new
  structural assertion) + 1 new concurrency + 9 conformance, no regressions.

Runner note: `cargo xtask` exposes only whole-suite subcommands; the red→green
flip used `cargo test -p wyrd-chunkstore-fs --test concurrent_put` (bounded by the
tool timeout, no hang risk). The authoritative gate `./engine/xtask.sh ci` is
Check's to run; locally `cargo fmt -p wyrd-chunkstore-fs -- --check` and `cargo
clippy -p wyrd-chunkstore-fs --all-targets -- -D warnings` both pass, so the patch
is commit-ready for the target's hooks. **Caveat for the C4 gate:** `cargo xtask
ci` also runs the unrelated flaky `durability_emission` DST custodian test (see
root-cause above); an intermittent red there is pre-existing and not this patch —
reproduce on `main` before attributing.

## Citations (post-patch line numbers in `$PDCA_WORKTREE`)

- `crates/chunkstore-fs/src/lib.rs:17` — `use std::sync::atomic::{AtomicU64, Ordering};`
- `crates/chunkstore-fs/src/lib.rs:36,47` — per-store `scratch_seq` field + init
- `crates/chunkstore-fs/src/lib.rs:42-58` — `open` (init + reap, ADR-0034 cited)
- `crates/chunkstore-fs/src/lib.rs:73-78` — unique `temp_path` (per-store seq)
- `crates/chunkstore-fs/src/lib.rs:88-111` — `reap_stale_temps`
- `crates/chunkstore-fs/src/lib.rs:150-159` — write→rename + own-scratch cleanup
- `crates/chunkstore-fs/src/lib.rs:306-308` — `scratch_file_name`
- `crates/chunkstore-fs/src/lib.rs:314-316` — `is_temp_scratch_name`
- `crates/chunkstore-fs/src/lib.rs:369` — structural uniqueness unit test
- `crates/chunkstore-fs/tests/concurrent_put.rs` — the brief-named regression test
- `docs/design/adr/0034-d-server-disk-model.md:52` — Model A, one D server per disk
- `madsim-0.2.34/src/sim/runtime/builder.rs:30,65` — madsim seed defaults to wall-clock
- `crates/dst/tests/custodian.rs:928-931,956-970` — the pre-existing flaky test (in-memory stores)
