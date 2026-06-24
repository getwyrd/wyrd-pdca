# Run FsChunkStore's disk I/O off the reactor

## Summary

Under sustained storage load a D server could silently disappear from discovery
and stop serving. The filesystem chunk store's async methods ran their disk
syscalls directly on the server's tokio reactor threads, so a burst of writes —
or one large `list_fragments` walk — could starve the lease-renew heartbeat;
when the lease then lapsed, the server was dropped from the node group. This
change moves the store's blocking filesystem work onto tokio's blocking pool so
the reactor (and its heartbeat) keeps running regardless of how much storage I/O
is in flight.

## What to look at

- `crates/chunkstore-fs/src/lib.rs` — the new `offload` helper and the five async
  `ChunkStore` methods (`put_fragment`, `get_fragment`, `list_fragments`,
  `delete_fragment`, `health`) now hand their `std::fs` calls to it. Cheap work
  (path joins, the post-read integrity check) stays on the reactor; only the
  syscalls move off it.
- To exercise it: `cargo test -p wyrd-chunkstore-fs` runs the new
  reactor-liveness test on a single-worker runtime. The existing
  `cargo test -p wyrd-server --test dserver` (incl.
  `lease_renews_and_lapses_deterministically`) covers the real-runtime host path.
- Ordering: this builds on the finalized write path (#203) and read/corruption
  contract (#207) in the same file and is intended to merge after both, so it
  wraps their bodies rather than re-deriving them through a changed dispatch.

## Root cause

`FsChunkStore` implements the **async** `ChunkStore` trait, but its method bodies
are **synchronous blocking** `std::fs` calls. The d-server hosts the store on a
multi-threaded tokio runtime (`crates/server/src/cli.rs:272`,
`new_multi_thread().enable_all()`) and runs the gRPC server and the lease-renew
heartbeat together in one `tokio::select!` on that runtime
(`crates/server/src/dserver.rs:164-180`); a storage RPC that blocks a worker
thread for its whole syscall therefore starves the heartbeat, and a missed renew
past the lease TTL drops the server out of discovery.

## Fix

Add an `offload` helper and route every blocking-syscall body of the async trait
through it, so the disk work runs on tokio's blocking pool instead of a reactor
worker thread. The helper is runtime-agnostic by design: it engages only inside a
live tokio runtime (production), and off one — the pollster- and madsim-driven
tests — the work runs inline exactly as before, so no existing test changed. As a
companion to the offload, `put_fragment` no longer stats/creates the chunk
directory on every write: it attempts the scratch write first and creates the
directory only on the genuine first-fragment miss, leaving the unique-scratch +
atomic-rename publish semantics unchanged.

## Verification

- **Claim:** No blocking filesystem syscall in the store's async `ChunkStore`
  methods runs on a reactor worker thread, so a co-scheduled timer/heartbeat keeps
  making progress while a storage-I/O burst is in flight.
- **Checked:** `crates/chunkstore-fs/src/lib.rs:152` — the `offload` helper runs
  the closure on tokio's blocking pool when a runtime is present and inline
  otherwise; routed at `:195` (`put_fragment`), `:236` (`get_fragment`), `:270`
  (`list_fragments`, the O(N) walk), `:308` (`delete_fragment`), `:322`
  (`health`). The starvation path it removes is at
  `crates/server/src/cli.rs:272` (multi-thread runtime) and
  `crates/server/src/dserver.rs:164-180` (lease renew co-scheduled with `serve`).
- **Test:** `crates/chunkstore-fs/tests/blocking_offload.rs` — on a single-worker
  runtime, a 1 ms timer task is co-scheduled with a burst of `list_fragments`
  walks and the test asserts the timer ticks **during** the burst. Pre-fix the
  blocking walk pins the lone worker and the timer makes 0 ticks (red); post-fix
  the offloaded walk frees the worker and the timer interleaves (green). The
  0-vs-many gap keeps the threshold off timing noise.
- **Supplementary (not part of this gate):** the end-to-end confirmation that the
  heartbeat stays live under real production storage load is observable on a live
  D server / load run.

Fixes #204
