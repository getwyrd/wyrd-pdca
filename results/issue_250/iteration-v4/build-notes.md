# Build notes — issue 250 / tier1-jepsen-consistency-harness (iteration 4)

## Root cause of prior rejections

**Iteration 1** rejected: Read primitive missing (no `wyrd ls`), hardcoded ports, repair
path never driven, nemesis not wired to actual compose containers, inverted self-test check.

**Iteration 2** rejected: Partition nemesis absent, custodian step treated any `reconcile_step`
success as pass (including `Reconciled::Satisfied`), failed reads recorded as `:ok`.

**Iteration 3** rejected: Two issues:
1. **T5**: `:concurrency 1` means a single sequential process — Elle sees a trivial serial
   history with no concurrent interleaving to verify. "Did I read back what I wrote" is not
   a meaningful consistency check.
2. **Item 5**: `(rand-int 1000000)` as appended value — Elle's list-append requires unique
   values within each slot; random ints can collide over 5 slots × run length.

## Iteration 4 changes vs iteration 3

The Rust/YAML/Cargo/Clojure self-test structure is identical to iteration 3 (which passed
C4-ci and C4-verify). Only `jepsen/src/wyrd/jepsen.clj` changes:

### Fix 1: `:concurrency 5` (from `:concurrency 1`)

**Location**: `jepsen/src/wyrd/jepsen.clj` in `wyrd-test`

**Why concurrency 1 was vacuous**: With a single Jepsen worker, operations are sequential.
Elle's list-append checker with a completely serial history is trivially satisfied: there are
no concurrent observations to check for consistency. The history is "process 0 appends A,
reads [A], appends B, reads [A,B], ..." — no interleaving, no ambiguity.

**Why concurrency 5 is non-vacuous**: With 5 workers (one per slot on average), reads
GENUINELY OVERLAP with in-flight appends. A read that starts while append(seq=7) is in
progress (waiting for D-server gRPC writes) reads the atom snapshot without seq=7, while a
read that starts after sees seq=7. Jepsen records these as temporally concurrent (overlapping
`:invoke`→`:ok` windows). Elle verifies: are all concurrent observations consistent with some
serialization?

The redb single-writer lock: concurrent `wyrd put` calls from multiple workers DO serialize
through the redb file lock (one waits, not fails). This means:
- Concurrent appends succeed, just queued through the file lock
- Concurrent reads run freely (redb multiple-reader semantics)
- The KEY concurrent interleaving is: reads start while appends are in-flight (waiting for
  D-server gRPC, not just redb), creating genuine overlapping history windows

**Rejected alternative — redesigning the workload model**: The sign-off said "redesign IF
single-writer redb is a hard constraint." It's not: redb serializes (blocks) rather than
rejecting concurrent writers. Increasing concurrency is simpler and directly addresses the
stated concern without redesigning the consistency model.

### Fix 2: Use `seq` as appended value (not `(rand-int 1000000)`)

**Location**: `invoke!` method in `WyrdClient`, `:append` case

**Before** (iteration 3): `v = (rand-int 1000000)` generated in `workload-ops`, used as
both the Wyrd value and the Elle list element.

**After** (iteration 4): The already-allocated `seq` (per-slot monotonic counter, claimed
by `alloc-seq!` at the start of the append) is used as BOTH the key suffix AND the stored
value. The generator now passes `nil` as a placeholder.

**Why this matters**: With 5 slots and e.g. 200 appends at rand-int 1000000, duplicate
values within a slot have ~2% probability (birthday problem). Duplicates make Elle's
list-append model ambiguous: if slot-0 has [3, 7, 3], is the second 3 a re-read of the
first, or a new write? Elle may produce false positives or fail to detect real anomalies.

With `seq` as value: slot-0's list is always [0, 1, 2, 3, ...] — unique, ordered,
unambiguous. The history is well-formed for Elle's list-append checker.

**Rejected alternative — using random unique IDs**: Would require a global uniqueness
generator (UUID or atomic counter) shared across all workers. The per-slot seq already gives
this for free and is already allocated in the append path.

## Proof of invariant restoration

The "deferred ≠ unbuilt" invariant from #146:

1. **Dispatch rewire** (`xtask/src/faults.rs:run_jepsen`): No longer routes to
   `WYRD_TIER1_JEPSEN_CMD` (nonexistent). Dispatches to `run_jepsen_harness()` which
   runs `lein run test` in `jepsen/` (the in-repo harness). Mirrors `run_disk_faults` →
   `run_tier1_scenario` pattern exactly.

2. **In-repo harness** (`jepsen/`): A real Clojure/lein project with `project.clj`,
   `jepsen/src/wyrd/jepsen.clj` (full Jepsen+Elle harness with crash+partition nemeses,
   5-worker concurrent workload, list-append Elle check), `jepsen/test/wyrd/checker_test.clj`
   (self-test: planted anomaly that Elle catches, clean history that Elle accepts).

3. **Privileged CI job** (`.github/workflows/tier1-jepsen.yml`): Nightly at 02:00 UTC
   (non-colliding: 03:00=tier1-disk-faults, 04:00=integration-nightly+mutants,
   05:00=tier2-kill-reconstruct) + `workflow_dispatch`, `WYRD_TIER1=1`, Docker cluster,
   kept OUT of unprivileged `cargo xtask ci` (ADR-0016).

## C4-ci evidence

- `cargo xtask ci` PASSES: fmt, clippy, build, test (including `jepsen_custodian_step.rs`
  born-at-tier unit tests + `jepsen_orchestration.rs` harness-dir tests + `faults.rs`
  dispatch rewire test), deny, conformance, DST.
- Specifically: `faults::tests::jepsen_dispatch_targets_in_repo_harness_not_env_cmd`,
  `jepsen_harness_dir_{lives_under_workspace_root,uses_provided_workspace_root,
  never_consults_env_var}`, `jepsen_topology_*` — all GREEN.

## C4-verify evidence (RED→GREEN)

`run-verify.sh` outcome:
- GREEN: both added test files (`xtask/tests/jepsen_orchestration.rs`,
  `crates/chunkstore-grpc/tests/jepsen_custodian_step.rs`) compile and pass with the fix.
- RED: reverting production files (while keeping test files) causes compile errors:
  - `wyrd_metadata_redb` unresolved (dev-dep removed from Cargo.toml)
  - `xtask::jepsen` module missing (jepsen.rs + lib.rs entry removed)

## What the deferred verification tier covers (off-Check)

The Clojure harness substance (live cluster + nemesis + Elle) is NOT exercised by C4-ci
(ADR-0016, accepted Option-A tradeoff). The first off-Check evidence:
1. `lein test` in `tier1-jepsen.yml` — runs `checker_test.clj` (planted anomaly must be
   caught by Elle; clean history must pass). This is the "demonstrated red" the self-test
   provides.
2. `cargo xtask jepsen` in `tier1-jepsen.yml` — runs `lein run test` against a live 5-node
   D-server cluster, injecting crash (kill+restart with tmpfs data loss) and partition
   (docker network disconnect) nemeses, with 5 concurrent workers, and Elle verifying the
   list-append history. The custodian reconstruction step is exercised after each crash heal.

## Files changed

| File | Change |
|------|--------|
| `xtask/src/faults.rs` | Rewire `run_jepsen`: dispatch to `run_jepsen_harness()` (not env-var external cmd); add `jepsen_harness_dir` helper; mark `execute`/`run_shell` `#[cfg(test)]`; add dispatch unit test |
| `xtask/src/jepsen.rs` | New: exports `jepsen_harness_dir` as library-target public fn |
| `xtask/src/lib.rs` | Add `pub mod jepsen` |
| `xtask/tests/jepsen_orchestration.rs` | New: Check-time flippable tests for harness-dir helper |
| `crates/chunkstore-grpc/Cargo.toml` | Add `wyrd-metadata-redb` dev-dependency |
| `crates/chunkstore-grpc/tests/jepsen_custodian_step.rs` | New: production custodian reconstruction step for live Jepsen cluster |
| `jepsen/project.clj` | New: Clojure/lein project definition |
| `jepsen/docker-compose.yml` | New: 5-node D-server cluster with tmpfs |
| `jepsen/src/wyrd/jepsen.clj` | New: Full Jepsen+Elle harness; **iteration-4 changes**: `:concurrency 5`, use `seq` as value |
| `jepsen/test/wyrd/checker_test.clj` | New: Elle self-test (planted anomaly + clean history) |
| `.github/workflows/tier1-jepsen.yml` | New: privileged CI job (nightly 02:00 UTC + dispatch) |
| `Cargo.lock` | Updated: `wyrd-chunkstore-grpc` entry now includes `wyrd-metadata-redb` |
