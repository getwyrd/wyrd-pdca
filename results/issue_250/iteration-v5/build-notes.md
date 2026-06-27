# Build notes — issue #250 Tier-1 Jepsen consistency harness (iteration 5)

## What this iteration fixes (carry-forward from iteration 4)

Three advisory items from the iteration-4 sign-off blocked acceptance.

### Fix 1: Data integrity violations fail the run (jepsen.clj:317 / :341)

**Problem (iter-4):** When `wyrd get` returns the wrong value for a key (corruption or
stale read), the harness threw `(ex-info "data integrity violation" ...)`. This was
caught by the broad `(catch Exception e)` block at line 341 and returned as `:fail`.
Jepsen records `:fail` as an availability failure, which Elle ignores. A corruption
that Wyrd incorrectly satisfied was therefore masked and would never cause the test to fail.

**Fix:** Changed the throw to `(AssertionError. ...)`. `AssertionError` extends
`java.lang.Error` (not `java.lang.Exception`), so it bypasses `(catch Exception e)`.
The error propagates out of `invoke!`, and Jepsen terminates the run as an unhandled
error. This is the correct behaviour: data corruption is not an availability blip but
a correctness failure that must be loud.

**Location:** `jepsen/src/wyrd/jepsen.clj` in the `:r` branch of `invoke!`.

**Rejected alternative (add `:integrity-violation` key to ex-data and re-throw in catch):**
Would add ~5 lines and work, but obscures intent. The `AssertionError` idiom is
idiomatic for "this should never happen; terminate immediately" in Clojure/Java code
and requires no added logic in the catch block.

### Fix 2: List order from completion order, not seq allocation (jepsen.clj:188 / :884)

**Root cause (iter-4):** The `:r` op sorted the slot history by `:seq` (the sequence
number allocated *before* `wyrd-put!` began). With `:concurrency 5`, two puts to the
same slot could have their seqs allocated in order [3, 4] but complete in order [4, 3].
This created two problems:

1. **Vacuous history:** Since all readers sort by seq, every reader sees the same order
   [3, 4] regardless of actual completion order. Elle has no variation to analyze.
2. **False positives:** A reader between B's completion (only seq 4 written so far)
   and A's completion (seq 3 written) sees [4] (only 4 in slot-writes at that point).
   A later reader sees [3, 4] (seq-sorted). Elle's list-append model: [4] followed by
   [3, 4] means 3 is *before* 4 in list order — but that contradicts seeing only 4.
   A spurious anomaly is reported even when Wyrd is correct.

**Fix:** Added a `slot-positions` atom and `alloc-position!` function. Positions are
allocated *after* a successful `wyrd-put!` returns. This records the actual completion
order from the Jepsen workers' perspective.

- `alloc-seq!`: allocated *before* put (unchanged) — unique key suffix for the put.
- `alloc-position!`: allocated *after* put success — completion-order position.
- `record-write!`: stores `{:pos N :key K :val N}` (was `{:seq N ...}`).
- `slot-history`: sorts by `:pos` (completion order) instead of `:seq`.

With this fix:
- Worker B (seq=4) completes first → pos=0, records (val:4, pos:0)
- Worker A (seq=3) completes second → pos=1, records (val:3, pos:1)
- slot-history sorted by pos: [4, 3]
- Reader R1 (between B and A): sees [4]
- Reader R2 (after both): sees [4, 3]
- [4] is a valid prefix of [4, 3] → Elle accepts (consistent history, no false positive)
- R1 and R2 see *different* observations → non-vacuous history Elle can analyze

**Rejected alternative (seq-as-completion-signal):** There is no way to know the
completion order from seq numbers alone (seq is allocated pre-put). A wall-clock
timestamp would have 1ms resolution collisions. The position counter is the cheapest
correct approach: one atomic swap per successful put.

**Why this doesn't fix T5 "can Elle detect real Wyrd anomalies?"** (NEEDS-HUMAN)
The list order still comes from the *client's view* of completion order, not from
Wyrd's internal state. A Wyrd bug that reorders put visibility without corrupting
values would not be directly visible in the position-sorted list. However: (a) wrong
values are now caught by Fix 1 (run termination), (b) unavailable keys are caught as
`:fail`, (c) the harness is no longer vacuous (readers do see different snapshots) and
no longer false-positive-prone. T5 is the maintainer's judgment item: "does the
workload yield non-vacuous Elle histories?" — this fix addresses the structural
objection (false positives + vacuous ordering) while acknowledging the fundamental
limitation of a CLI-wrapper harness.

### Fix 3: Docker network disconnect failures propagate (jepsen.clj:453 / :490)

**Problem (iter-4):** `disconnect-network!` and `reconnect-network!` logged nonzero
docker exits as warnings but did not throw. The `partition-nemesis/invoke!` still
returned `:info partitioned` even when the actual disconnect failed (e.g., wrong
network name `wyrd-jepsen_default` vs the actual network). The Jepsen run would
proceed believing a partition had been injected when none had — a vacuous
partition-resilience test.

**Fix:** Both `disconnect-network!` and `reconnect-network!` now throw `ex-info` on
nonzero docker exit. The exception propagates from `partition-nemesis/invoke!` (not
caught there), causing Jepsen to record a nemesis failure and terminate the run. The
teardown method remains best-effort (only logs) to avoid masking the real error.

**Error message:** Includes the network name and container name so the operator knows
exactly what was tried and can fix `WYRD_JEPSEN_NETWORK`.

## Files changed

| File | Change |
|------|--------|
| `xtask/src/faults.rs` | Rewire `run_jepsen` → `run_jepsen_harness()` (lein run test in jepsen/); mark `execute`/`run_shell` `#[cfg(test)]`; add `jepsen_harness_dir`; add `jepsen_dispatch_targets_in_repo_harness_not_env_cmd` test |
| `xtask/src/jepsen.rs` | NEW — library target: pure `jepsen_harness_dir` helper for Check-time tests |
| `xtask/src/lib.rs` | Add `pub mod jepsen;` |
| `xtask/tests/jepsen_orchestration.rs` | NEW — Check-time flippable tests for `jepsen_harness_dir` |
| `jepsen/project.clj` | NEW — Clojure/lein project with Jepsen 0.3.7 + Elle 0.2.2 |
| `jepsen/src/wyrd/jepsen.clj` | NEW — Main harness with all three iteration-5 fixes |
| `jepsen/test/wyrd/checker_test.clj` | NEW — Elle self-test (planted version-cycle anomaly) |
| `jepsen/docker-compose.yml` | NEW — D-server cluster with tmpfs /data per replica |
| `crates/chunkstore-grpc/Cargo.toml` | Add `wyrd-metadata-redb` dev-dep |
| `crates/chunkstore-grpc/tests/jepsen_custodian_step.rs` | NEW — Rust custodian reconstruction step (born-at-tier topology tests + #[ignore]'d scenario) |
| `.github/workflows/tier1-jepsen.yml` | NEW — Privileged nightly/dispatch CI job at 02:00 UTC |
| `Cargo.lock` | Updated: `wyrd-metadata-redb` added to `wyrd-chunkstore-grpc` deps |

## Test strategy

### Check-time (cargo xtask ci — red→green)

**Test:** `jepsen_dispatch_targets_in_repo_harness_not_env_cmd` in `xtask/src/faults.rs:397`
- RED pre-fix: `jepsen_harness_dir` doesn't exist → compile error
- GREEN post-fix: asserts harness dir is `<workspace>/jepsen` (never the WYRD_TIER1_JEPSEN_CMD env var)

**Tests:** `jepsen_harness_dir_*` in `xtask/tests/jepsen_orchestration.rs`
- RED pre-fix: `xtask::jepsen` module doesn't exist → compile error
- GREEN post-fix: path helper works correctly for any workspace root

**Tests:** `jepsen_topology_*` in `crates/chunkstore-grpc/tests/jepsen_custodian_step.rs`
- Born-at-tier tests (not `#[ignore]`d): run in `cargo test --workspace`
- Verify 5-server topology has distinct domains A–E

### Off-Check (tier1-jepsen.yml — deferred by design)

**Elle checker self-test (`lein test`):**
- `clean-history-passes-elle`: Elle accepts consistent two-append history (`:valid? = true`)
- `anomalous-history-fails-elle`: Elle rejects planted version-cycle anomaly (`:valid? = false`)

**Full Jepsen run (`cargo xtask jepsen`, WYRD_TIER1=1):**
- Crash nemesis + partition nemesis against live 5-node cluster
- Elle list-append checker verifies strict serializability
- Custodian reconstruction step drives production `reconcile_step` path

## Red→green verification

Ran `./engine/xtask.sh ci` in the PDCA worktree (`$PDCA_WORKTREE`):
```
xtask ci: all checks passed
```

The `jepsen_dispatch_targets_in_repo_harness_not_env_cmd` and all `jepsen_harness_dir_*`
tests are green post-fix, absent/compile-error pre-fix (born-at-tier RED by criterion-absence).

## Design decisions

### `#[cfg(test)]` on `execute` and `run_shell`
These functions are only called from the test suite (not from any production runner path
after the `run_jepsen` rewire). Marking them `#[cfg(test)]` avoids a dead_code warning
from clippy. They remain accessible to the existing tests in `mod tests`. Cost: 2 lines
of annotation. Alternative (delete the tests) would lose the existing coverage for the
generic `execute` machinery, which still validates the deferred/missing-tool behaviour
shared by all runners — kept.

### Completion-order position vs. value-based ordering
The natural alternative for non-vacuous list ordering would be to have Wyrd store an
ordered list directly (e.g., one key per slot containing a comma-separated list of
values, updated via CAS). This would give true "observed from Wyrd" ordering. However:
- The Wyrd CLI has no CAS command (and adding one is out of scope for this brief)
- A CAS-on-read-then-write approach would add failure modes under concurrent writes
- The completion-order position approach is testable without any Wyrd API changes

The completion-order approach represents the client's linearization point of each
append (the point at which the client observed the put as successful). This is a valid
and observable ordering for a consistency test, as long as we acknowledge that it
reflects the client's view rather than Wyrd's internal commit log. The T5 NEEDS-HUMAN
item at Check captures this limitation for the maintainer's judgment.

### `AssertionError` for data integrity violations
`AssertionError` (extends `Error`) is the standard Clojure idiom for "this cannot
happen; terminate immediately". It bypasses `catch Exception` blocks at all levels
of the call stack. The alternative (a custom `deftype` exception that we re-throw)
would add 10+ lines and introduce a custom type that's harder to recognize in
Jepsen's error logs. `AssertionError` with a clear message is self-documenting.
