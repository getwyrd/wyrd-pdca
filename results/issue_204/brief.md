# Brief — issue 204 / fschunkstore-offload-blocking-io

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** fschunkstore-offload-blocking-io
- **Defect:** `FsChunkStore` implements the **async** `ChunkStore` trait with
  **synchronous blocking `std::fs`** calls executed directly inside the `#[async_trait]`
  method bodies — `use std::fs;` (`crates/chunkstore-fs/src/lib.rs:14`) and
  `fs::create_dir_all/write/rename/read/read_dir/remove_file/metadata` at `:33, :78-79,
  :82, :88, :99-136, :141, :149` — with no `spawn_blocking`, `block_in_place`, or
  `tokio::fs`. The d-server hosts the store on a **multi-threaded** tokio runtime
  (`crates/server/src/cli.rs:272`, `new_multi_thread().enable_all()`) and dispatches each
  RPC on its own task over an `Arc<store>` (`crates/chunkstore-grpc/src/server.rs:33-36,
  :46-60`). Every storage RPC therefore blocks a tokio **worker thread** for its whole
  disk syscall: I/O parallelism is capped at the worker-thread count and, while those
  threads sit in syscalls, the reactor runs nothing else. Critically, `DServer::serve`
  runs the gRPC server and the lease-renewal heartbeat in one `tokio::select!` on the same
  runtime (`crates/server/src/dserver.rs:160+`); sustained worker-thread starvation — a
  concurrent-write burst or one multi-million-entry `list_fragments` walk — can delay the
  renew tick, and past the lease TTL the renewal is missed and the **server drops out of
  discovery and stops serving**.
- **Success criterion:** Under a sustained burst of concurrent storage I/O (writes and/or
  a long `list_fragments` walk), a co-scheduled timer/heartbeat task on the **same**
  runtime continues to make progress / fire on schedule instead of being starved while
  the I/O is in flight — i.e. the storage I/O no longer occupies the reactor's worker
  threads for the duration of its syscalls. Demonstrable at C4-verify by a behavioural
  test on a constrained runtime (see Verification posture). BINDING is "blocking
  filesystem syscalls no longer run on a reactor worker thread, so the reactor /
  heartbeat / other RPCs stay live under storage load"; the offload mechanism
  (`tokio::task::spawn_blocking` around the `std::fs` calls vs. switching to `tokio::fs`)
  is ILLUSTRATIVE — Do's call. The `list_fragments` walk is explicitly included in the
  offload (its O(N) cost is the worst starvation source).
- **Invariant to restore:** No blocking filesystem syscall in the storage backend's async
  `ChunkStore` methods may execute on a tokio **reactor worker thread**; blocking I/O is
  performed off the reactor (on the blocking pool / a thread that may block) so the
  runtime's reactor — its accept loop, its timers including the lease-renew heartbeat, and
  every other in-flight task — stays schedulable independent of how many storage syscalls
  are in flight. Source: the Tokio runtime contract — "async code should never spend a
  long time without reaching an `.await`; blocking the executor thread starves other
  tasks", and the documented remedies `spawn_blocking` / `tokio::fs` (tokio docs for the
  version pinned in `Cargo.lock`; Tier B framework canon, authoritative — confirm the
  pinned version). (Structural fix — *where work runs* per principles.md §1.2; target is
  the smallest change that restores the non-blocking-reactor invariant across **all** of
  the store's I/O methods, not the smallest diff. Self-test: the invariant is over every
  blocking syscall in the backend's async path, so a patch that offloads only one method
  visibly fails it.)
- **Repo + branch target:** getwyrd/wyrd @ main   (resolve here at Plan — do not leave to Do)
- **Depends on (merged):** 203, 207   (this brief is the structural I/O rewrite of
  `crates/chunkstore-fs/src/lib.rs` and lands LAST in that file's merge chain, wrapping the
  finalized write and read bodies the other two briefs produce — building on their
  **merged** result avoids a merge conflict across the rewritten I/O dispatch; both
  predecessors' PRs must be merged before this one's Do runs)
- **Surfaces:** data

> **Merge chain — `crates/chunkstore-fs/src/lib.rs`: 203 → 207 → 204.** This brief is the
> tail: it restructures *how* every method does I/O, so it must wrap the final
> `put_fragment` (unique temp, 203) and `get_fragment` (corruption-error contract, 207)
> bodies rather than have those re-applied through a changed dispatch. It is therefore held
> until both predecessors' PRs are merged. (Order is a planning choice — the only hard
> requirement is a single acyclic merged-chain over this file; 204-last keeps the
> hardest-to-verify structural change off the critical path of the two durability fixes.)
- **Scope:** move the filesystem store's blocking `std::fs` I/O (including the
  `list_fragments` walk) off the reactor so reactor-resident tasks are no longer starved
  by storage syscalls; and skip the per-write `create_dir_all` on the steady-state path —
  `put_fragment` calls `fs::create_dir_all(chunk_dir)` on **every** write (`:78-79`),
  a stat (+maybe mkdir) for a directory that exists after the first fragment — so the
  hot path attempts the write/rename and only creates the directory when it is genuinely
  absent. / out of scope: the gRPC server concurrency-limit / backpressure / HTTP-2
  tuning (#205); the unique-temp-per-write race fix (#203); `fsync` durability; the
  `get_fragment` corruption-error contract (#207); the *materialization/streaming* of
  `list_fragments` at scale (tracked under ADR-0032's at-scale layout, not here). Sizing
  the blocking pool / I/O concurrency to the device (HDD seek-storms vs. NVMe deep
  queues) is desirable to document and keep tunable, but it is the companion concern of
  #205 — keep any default here conservative and not load-bearing for this fix's criterion.
- **Repro instruction:** On `main` @ `c2223a5`, on a tokio runtime constrained to a small
  worker-thread count, co-schedule (a) a tight periodic timer/heartbeat task and (b) a
  burst of `FsChunkStore` storage operations (many concurrent writes and/or a large
  `list_fragments` over a populated store, with the syscalls made observably slow).
  Observe the timer task's ticks are delayed/skipped while the storage syscalls occupy
  the worker threads (pre-fix), reproducing the heartbeat-starvation path.
- **Test file:** crates/chunkstore-fs/tests/blocking_offload.rs   (net-new; a co-scheduled
  timer task keeps ticking while a slow storage-I/O burst runs — see Verification posture)
- **Verification posture:** the binding property is a reactor-liveness/timing behaviour,
  not a clean value flip. Make it demonstrable by constraining the runtime to **one**
  worker thread and asserting a co-scheduled async task makes progress *while* a
  deliberately-slow store operation is in flight: pre-fix the blocking syscall pins the
  single worker so the co-scheduled task cannot run until I/O completes (demonstrated
  red); post-fix the offloaded I/O leaves the worker free and the task progresses (green).
  Do should engineer the slow-I/O determinism (e.g. a store operation over enough entries,
  or a seam that makes a syscall observably block) rather than rest on real-disk timing.
  If a reliably-flipping deterministic red is impractical at C4-verify, record why (per
  CLAUDE.md "no test because X") plus a manual repro, and assert the structural invariant
  (no `std::fs` on the async path) as the durable check. The deferred-green confirmation
  (heartbeat stays live under real production load) is observable off-Check, on a live
  d-server / load run — name it as supplementary, not the binding C4 evidence.
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched `crates/chunkstore-fs/src/lib.rs` and
  `crates/server/src/cli.rs`/`dserver.rs` across merged history (`f428ec7`, `093732d`,
  `4cd77d2`, `186c82f`, `17cfb91` — all use blocking `std::fs` directly; none offloaded
  it), open PRs (`gh pr list --state open` — none touch these files), and closed PRs — no
  prior or in-flight fix.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
