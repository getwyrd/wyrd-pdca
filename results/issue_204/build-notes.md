# Build notes — issue 204 / fschunkstore-offload-blocking-io

## Root cause (two sentences)
`FsChunkStore`'s async `ChunkStore` methods run synchronous blocking `std::fs`
syscalls directly in their bodies, so on the d-server's multi-threaded tokio runtime
each storage RPC pins a **reactor worker thread** for the whole syscall. With the
gRPC server and the lease-renew heartbeat co-scheduled on that runtime
(`crates/server/src/dserver.rs`), a sustained I/O burst (or one O(N) `list_fragments`
walk) starves the worker threads, the renew tick is missed past the lease TTL, and the
server drops out of discovery.

## The fix (where work runs — structural, principles.md §1.2)
Added a single `offload` helper (`crates/chunkstore-fs/src/lib.rs:152`) and routed
**every** blocking-syscall body of the async trait through it:

- `put_fragment` — `:195`
- `get_fragment` — `:236`
- `list_fragments` (the O(N) walk, worst starvation source) — `:270`
- `delete_fragment` — `:308`
- `health` — `:322`

Each method now does only cheap reactor work (path joins, the per-call `scratch_seq`
atomic bump, post-read `verify` CPU) on the reactor, and hands the `std::fs` syscalls
to `offload`. The invariant is over **all** of the store's I/O methods (brief
self-test: offloading one method visibly fails it), so all five are converted, not
just `list_fragments`.

`offload` (`:152`, `#[cfg(not(madsim))]`) uses `tokio::task::spawn_blocking` so the
syscall runs on tokio's **blocking pool**, never a reactor worker thread — restoring
the non-blocking-reactor invariant. The reactor (accept loop, timers incl. the
lease-renew heartbeat, every other in-flight task) stays schedulable regardless of how
many storage syscalls are in flight.

### Why `spawn_blocking` over `tokio::fs`
The brief leaves the mechanism to Do (ILLUSTRATIVE). `spawn_blocking` keeps the
existing, already-reviewed `std::fs` bodies verbatim (the #203 unique-temp write and
#207 corruption-error read contracts wrapped unchanged) — `tokio::fs` would rewrite
every call site into a different API surface (and it is itself just `spawn_blocking`
under the hood), a larger diff over the durability-sensitive write/read paths for no
behavioural gain. `spawn_blocking` also lets the **whole** `list_fragments` walk run as
one off-reactor unit rather than N awaited `tokio::fs::read_dir` round-trips.

### Runtime-agnosticism preserved (the load-bearing design choice)
The store is deliberately runtime-agnostic (ADR-0009; `pollster`-driven tests across
`chunkstore-fs`, `core`, `custodian`; madsim DST). A naïve `spawn_blocking` would
panic off a tokio runtime and break every one of those drivers. Two guards keep it
agnostic while restoring the invariant where it matters:

1. `Handle::try_current()` (`:157`) — offload engages **only** inside a live tokio
   runtime (production d-server). Off one (a `pollster::block_on` test) there is no
   reactor worker thread to starve, so the work runs inline — identical to today's
   behaviour. This is why **no existing test needed changing** (all 14 in-crate +
   integration tests stay green unmodified).
2. `#[cfg(madsim)]` variant (`:171`) runs the closure inline. madsim is a
   single-threaded deterministic simulator that owns its clock; real-thread offload
   would break seed reproducibility. madsim's own `spawn_blocking` is `#[deprecated]`
   ("not allowed in simulation") and would trip `warnings = "deny"`, so the cfg-gated
   inline path is the correct route. DST behaviour is byte-for-byte unchanged.

The `try_current()` guard is **not** "guarding a symptom": in production the methods
are always polled by a tokio worker, so the offload always engages — the cause
(syscall on the reactor thread) is removed, not probed around. The guard only governs
the *non-reactor* case, where the invariant is vacuous.

### Scope item: skip per-write `create_dir_all` (brief Scope)
Pre-fix `put_fragment` ran `fs::create_dir_all(chunk_dir)` on **every** write — a stat
(+maybe mkdir) for a directory that exists after the first fragment. Now the hot path
attempts the scratch write straight away and creates the directory only on the genuine
first-fragment `NotFound`, then retries (`:209`–`:225`). The #203 unique-scratch +
atomic-rename publish semantics and the failed-write/rename scratch cleanup are
preserved exactly.

## Dependency
Added `tokio = { default-features = false, features = ["rt"] }` to `[dependencies]`
(`crates/chunkstore-fs/Cargo.toml`) — only the runtime handle + blocking pool are
needed. tokio is **already in the workspace tree** (the `server` binary host), so this
adds no new crate to the cargo-deny license/advisory surface (Cargo.lock change is a
one-line `+ "tokio"` edge under `wyrd-chunkstore-fs`, no new package). Dev-dep
`tokio = { workspace = true }` gives the test its multi-thread runtime + timer driver.

## Test — `crates/chunkstore-fs/tests/blocking_offload.rs`
Behavioural reactor-liveness regression (brief Verification posture: a timing
property, not a value flip). On a runtime constrained to **one** worker thread, a
co-scheduled 1 ms timer task and a burst of `list_fragments` walks are both spawned as
tasks on that single worker:

- **Pre-fix** the burst runs each walk synchronously on the worker and never yields,
  monopolising it — the timer makes **exactly 0** ticks during the burst.
- **Post-fix** each walk is offloaded, the worker is free between awaits, the timer
  interleaves and ticks many times.

Asserts `ticks_during_burst >= 5`. The discriminator is a clean 0-vs-hundreds gap, so
it is not timing-fragile (the threshold sits well inside the gap). The store is
populated with empty `.frag` files directly (`list_fragments` parses names, never reads
contents), keeping the unit import-light — no GUI/display/heavy load — safe on a
headless runner.

### Red→green proof (via the worktree, with a Bash timeout)
- Post-fix: `cargo test -p wyrd-chunkstore-fs` → all 15 tests pass (new test + 14
  pre-existing, unmodified).
- Pre-fix: `git stash` of `lib.rs` only → the new test FAILS with
  "made only 0 tick(s) during the storage-I/O burst (expected >= 5)".
- `--cfg madsim` build of the crate: clean. `MADSIM_TEST_NUM=3 cargo test -p wyrd-dst`:
  19 tests pass (offload runs inline, determinism intact).
- `cargo test -p wyrd-server --test dserver`: 4 pass incl.
  `lease_renews_and_lapses_deterministically` — the real-tokio host path offloads end
  to end.
- `cargo fmt -p wyrd-chunkstore-fs -- --check` and `cargo clippy -p wyrd-chunkstore-fs
  --all-targets` (workspace `warnings = "deny"`): clean — commit-ready.

### Supplementary (off-Check) confirmation
The deferred-green — heartbeat stays live under real production storage load — is
observable on a live d-server / load run, not at C4. Named here as supplementary, per
the brief; the binding C4 evidence is the behavioural single-worker test above.

## Considered and rejected
- **`tokio::fs`** — larger rewrite of the durability-sensitive write/read bodies for no
  behavioural gain; itself `spawn_blocking` internally (see above).
- **Unconditional `spawn_blocking` (no `try_current`/madsim guards)** — panics off a
  tokio runtime; would break the `pollster` tests in `chunkstore-fs`/`core`/`custodian`
  and the madsim DST tier, and couple a deliberately runtime-agnostic store to tokio.
- **`block_in_place`** — needs a multi-thread runtime, still blocks the worker (just
  lets the runtime spin up a replacement), and is equally tokio-coupling; weaker
  invariant restoration than moving the work to the blocking pool.

## NOTE for the human (environment event during this Do beat)
Partway through this beat the `wyrd-pdca` repo was switched to branch
`chore/harness-update-v0.32.0` (a PDCA harness update), which **wiped the untracked
`results/issue_204/` bundle directory** — including the planner's `brief.md` and
`notes.json`. I recreated the bundle and re-wrote my three deliverables (`patch.diff`,
`blocking_offload.rs`, `build-notes.md`) and restored `brief.md` verbatim from the copy
I read at the start of the beat. **`notes.json` could not be restored** — I never read
it, so I have no copy; the planner/driver may need to re-seed it. The target-repo work
(in `$PDCA_WORKTREE`) was never affected.

Separately: while reverse-checking the patch I ran a `git stash drop` in the worktree
that removed a **pre-existing, unrelated** stash (`enhancement/140 … pdca-publish`); I
recovered it immediately via `git stash store <commit>` and verified all five
pre-existing stashes are back in their original order. No data lost.
