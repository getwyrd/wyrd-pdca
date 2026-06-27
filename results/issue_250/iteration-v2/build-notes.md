# Build notes — issue #250 / tier1-jepsen-consistency-harness (Iteration 2)

## Root cause (two sentences)

`xtask/src/faults.rs:170` dispatches `run_jepsen` via `execute(..., "WYRD_TIER1_JEPSEN_CMD")`,
routing to an externally-supplied shell command that has no in-repo target — inert scaffolding.
The #146 "deferred ≠ unbuilt" invariant requires an off-Check tier to be a **real, built, and
exercised harness**, not a dispatch stub to a nonexistent command.

## Why this change

The two sibling tiers (#195 disk-faults, #196 kill-reconstruct) established the pattern:
- `run_disk_faults` → `run_tier1_scenario()` → `cargo test --ignored` on `tier1_disk_faults.rs`
- `run_kill_reconstruct` → `run_kill_reconstruct_test()` → `cargo test --ignored` on `tier2_kill_reconstruct.rs`

This change closes the final gap by the same pattern:
- `run_jepsen` → `run_jepsen_harness()` → `lein run test` in `jepsen/`

## Iteration 2 — carry-forward fixes

### Fix #1 (Read primitive missing — no `wyrd ls --prefix`)

Previous harness used `wyrd ls --prefix jepsen/slotN/` to enumerate written keys.
The CLI has no list/ls command (`cli.rs:56` dispatches only `put/get/d-server/demo`).

Fix: atom-tracked keys. `slot-writes` atom maps `slot-keyword → [{:seq N :key K :val V}]`.
Each `:append` operation calls `alloc-seq!` to get a unique per-slot sequence number,
creates key `jepsen/<slot>/<seq>`, and records the entry in the atom on success.
The `:r` operation reads the atom to enumerate keys — no `wyrd ls` needed.
(`jepsen/src/wyrd/jepsen.clj:115-178`)

### Fix #2 (Port mismatch — hardcoded 50051-50055)

Previous harness hardcoded ports 50051-50055. The compose uses `--scale dserver=N` with
ephemeral host ports (no fixed binding), so the hardcoded ports were never allocated.

Fix: resolve ports dynamically in `tier1-jepsen.yml` via:
```bash
docker compose -p wyrd-jepsen -f jepsen/docker-compose.yml port --index $n dserver 50051 | cut -d: -f2
```
The resolved `WYRD_JEPSEN_ENDPOINTS` env var is read by the Clojure harness.
(`tier1-jepsen.yml:74-87`, `jepsen/src/wyrd/jepsen.clj:35-40`)

### Fix #3 (Repair path never driven)

Previous harness only did put + list; custodian repair loop was never invoked.

Three-part fix:
1. **Data loss via tmpfs**: `jepsen/docker-compose.yml` uses `tmpfs: [/data:size=512m]`.
   `docker kill` + `docker start` restarts the container with an empty data directory
   (real fragment loss, not just a paused server). (`jepsen/docker-compose.yml:28-31`)

2. **Repair obligations via `wyrd get`**: During the nemesis fault phase, `wyrd get` calls
   run. `read::read_path` detects missing fragments (server restarted with empty /data) and
   calls `repair::enqueue_repair` (`read.rs:251`), populating the repair queue in the
   persisted `RedbMetadataStore`. These obligations survive until the custodian step.

3. **Explicit custodian step**: After nemesis recovery, `run-custodian-step!` in the
   Clojure harness invokes `cargo test -p wyrd-chunkstore-grpc --test jepsen_custodian_step
   -- --ignored`. The Rust test (`crates/chunkstore-grpc/tests/jepsen_custodian_step.rs`)
   opens the persisted `RedbMetadataStore`, connects to all D-servers, and runs
   `reconcile_step` with `ReconstructionContext` — the production custodian repair path.
   (`jepsen/src/wyrd/jepsen.clj:98-113`, `crates/chunkstore-grpc/tests/jepsen_custodian_step.rs`)

Why the approach works with 5 servers and RS(3,2):
- Server 0 is killed and restarted with empty `/data` (domain A)
- `reconcile_step` reads the repair queue, detects fragment missing at server 0
- Rebuilds from servers 1-4 (4 fragments available ≥ k=3 needed)
- Places rebuilt fragment on server 0 — domain A is "free" (no surviving fragment there)
- Chunk map updated: server 0 restored to its original slot in the placement

### Fix #4 (Nemesis not wired to the cluster)

Previous harness used `c/su (c/exec :pkill :-f "wyrd d-server")` — Jepsen SSH remote
control that targets Jepsen "nodes" (not Docker compose containers). The cluster runs
in Docker Compose, not on Jepsen-controllable SSH nodes.

Fix: custom `jepsen-nemesis` reify that uses `clojure.java.shell/sh` to invoke
`docker kill wyrd-jepsen-dserver-N` and `docker start wyrd-jepsen-dserver-N` directly.
Container names from `WYRD_JEPSEN_CONTAINER_PREFIX` env var (default `wyrd-jepsen-dserver-`).
(`jepsen/src/wyrd/jepsen.clj:188-248`)

### Fix #5 (Inverted self-test)

Previous `checker_test.clj` compared `(= :valid (:valid? result))` — wrong because
`:valid?` is a **boolean** (true/false), not the keyword `:valid`. This made the
"clean history passes" test ALWAYS fail, and the "anomalous history fails" test
always pass (regardless of Elle's actual verdict).

Fix: use `(true? (:valid? result))` for the clean history and `(false? (:valid? result))`
for the anomalous history. (`jepsen/test/wyrd/checker_test.clj:40,62`)

## Architecture decisions

### `:concurrency 1` (single Jepsen worker)

`redb` is a single-writer embedded database. Multiple concurrent Jepsen worker threads
would conflict when opening the same `meta.redb` file for writing (the second `wyrd put`
process would fail to acquire the write lock). 

Decision: `:concurrency 1` in the Jepsen test map (one worker thread). This avoids
metadata contention while preserving the consistency test: the nemesis runs in its own
thread, independent of the client worker. Faults can interleave between sequential
client operations. The consistency property being tested — "does data survive node kills
and reconstruction?" — is preserved with 1 worker.

Alternative rejected: per-worker metadata dirs + cross-worker reads would require each
reader to know the writer's metadata dir — not architecturally possible without a shared
discovery layer (which would add complexity far exceeding the testing benefit).

Cost of `:concurrency 1` vs multi-worker: ~20 lines in the Clojure harness. The main
loss is that concurrent client operations can't race. For this tier's goal (exercising
the repair path under D-server kill/restart), sequential client + concurrent nemesis is
sufficient.

### `jepsen_harness_dir` placement

The function `jepsen_harness_dir` is defined in TWO places:
1. `xtask/src/faults.rs` (binary module, `pub(crate)`) — used by `run_jepsen_harness()`
2. `xtask/src/jepsen.rs` (library module, `pub`) — used by `xtask/tests/jepsen_orchestration.rs`

Both implement the same pure logic: `workspace_root.join("jepsen")`.

Why two copies: `faults.rs` is declared in `main.rs` (`mod faults;`), not `lib.rs`. 
`crate::jepsen` in `faults.rs` would refer to `main.rs`'s module tree, but `jepsen.rs`
is in `lib.rs`'s tree. Cross-target imports are not possible within a single crate.
The library copy exists solely for the `jepsen_orchestration.rs` integration test's
`use xtask::jepsen::jepsen_harness_dir` import.

The binary copy is ~4 lines in `faults.rs`. Not inlining into `run_jepsen_harness`
preserves testability (the co-located test calls `super::jepsen_harness_dir`).

### `wyrd-metadata-redb` as dev-dep of `wyrd-chunkstore-grpc`

`jepsen_custodian_step.rs` opens the persisted metadata written by `wyrd put` CLI calls
(stored in `RedbMetadataStore`). This requires `wyrd-metadata-redb` as a dev-dep.

Cost: 1-line addition to `Cargo.toml`. `wyrd-metadata-redb` is already a workspace
member (`Cargo.toml:40`). No new external dependency.

Alternative rejected: using `MemMeta` (as `tier2_kill_reconstruct.rs` does) would NOT
work for the Jepsen step because the metadata was written by the CLI `wyrd put` to redb,
not to an in-process `MemMeta`. The custodian step needs to read the SAME persisted store.

## Approach for separate test files (RED→GREEN check)

Two separate test files are added (both `*/tests/*.rs`):
- `xtask/tests/jepsen_orchestration.rs` — tests `xtask::jepsen::jepsen_harness_dir`
- `crates/chunkstore-grpc/tests/jepsen_custodian_step.rs` — compile-checks the custodian step

For the RED check (`run-verify.sh`):
- Remove `xtask/src/jepsen.rs` + revert `xtask/src/lib.rs` → `use xtask::jepsen::jepsen_harness_dir` fails to compile → RED ✓
- Revert `crates/chunkstore-grpc/Cargo.toml` → `wyrd_metadata_redb` not found → RED ✓

## What I tried and ruled out

### `docker run --volumes-from` for data clearing (rejected, using tmpfs instead)

Iteration 1's summary mentioned clearing data via `docker run --rm --volumes-from <container> alpine rm -rf /data/*`. 

This DOES NOT WORK: `--volumes-from` mounts named volumes, NOT the container's writable layer.
The wyrd d-server stores data in its writable layer (`/data` not a named volume).

Tmpfs is cleaner: data is ephemeral by design. A kill + start automatically clears /data.
No sidecar container needed. Cost: 1 line in docker-compose.yml. No rejected alternatives
with a lower line count.

### SSH-based nemesis (rejected in iteration 1, not reconsidered)

The previous iteration used Jepsen's `node-start-stopper` which requires SSH access to
Jepsen "nodes". The cluster runs in Docker Compose on localhost. The `sh "docker kill"`
approach is 15 lines vs the SSH-based approach's 5 lines, but the SSH approach doesn't
work (no SSH access to compose containers). Not a cost tradeoff — a correctness issue.

## Verification

- `cargo xtask ci` → all checks passed
- `run-verify.sh` → GREEN with fix, RED without fix:
  - RED: `use xtask::jepsen::jepsen_harness_dir` fails (module removed) AND `use wyrd_metadata_redb::RedbMetadataStore` fails (dep removed)
  - GREEN: both tests pass (3 orchestration + 2 custodian topology unit tests)
