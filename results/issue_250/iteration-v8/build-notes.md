# Build notes — issue #250 (tier1-jepsen-consistency-harness)

## What the brief required

Five bindable outputs:

1. `run_jepsen` in `xtask/src/faults.rs` dispatches to the in-repo Tier-1
   consistency scenario (not `WYRD_TIER1_JEPSEN_CMD`).
2. Dispatch wiring + opt-in gating unit-tested inside `cargo xtask ci`.
3. New buildable Rust scenario `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`
   that compiles/type-checks under `cargo xtask ci` (`#[ignore]`d body), driving
   production `custodian::reconcile_step` → `reconstruction::reconcile`.
4. New privileged CI job `.github/workflows/tier1-jepsen.yml` (nightly + workflow_dispatch,
   `WYRD_TIER1=1`).
5. Red→green: test goes RED when `jepsen.rs` is removed and `faults.rs` reverted,
   GREEN with full patch applied.

## What I did and why

### Iterations 1–7 carry-forward

Iterations 1–6 tried Clojure/Jepsen (Option A), which can't produce non-vacuous runs
against Wyrd's immutable-single-write-per-key store. Iteration 7 (first Option B) had
the single-source-of-truth problem: `run_jepsen_consistency_test` hand-typed its cargo
args instead of consuming `jepsen::consistency_test_cargo_args()`, so reverting `faults.rs`
left every routing test green.

### Fix: jepsen.rs as a shared lib+binary module

Added `xtask/src/jepsen.rs` with two categories of items:

**Dispatch items** (used by both binary and lib):
- `consistency_test_cargo_args()` → the single source of cargo args
- `consistency_required_tool()` → `"docker"` (replaces old `"lein"`)
- `JC_DSERVER_COUNT` = 7, `JC_VICTIM_INDEX` = 0

**Oracle items** (lib+integration-test only — false-positive dead-code in binary):
- `ConsistencyEvent`, `ConsistencyOutcome`
- `check_read_after_commit`, `check_no_duplicate_placement`

The module is included in BOTH the lib (`pub mod jepsen;` in `lib.rs`) and the binary
(`mod jepsen;` in `main.rs`). The binary uses only the dispatch items; the oracle items
carry `#[allow(dead_code)]` to suppress the false-positive from the binary's dead-code
pass.

**Why the oracle items stay in `jepsen.rs` rather than a separate `jepsen_oracle.rs`**:
Moving them to a lib-only file would eliminate the need for `#[allow(dead_code)]` (~4
lines), but would require splitting a conceptually unified module, changing the test
import path (`xtask::jepsen` vs `xtask::jepsen_oracle`), and adding a new public module
to `lib.rs`. The `allow` annotations are self-documenting (each says exactly why) and
the unified module is cleaner.

### faults.rs rewrite

Removed `execute()` / `run_shell()` and their two tests (dead code after rewire;
`warnings = "deny"` would fail them). Added:
- `run_jepsen()`: gates on `crate::jepsen::consistency_required_tool()` ("docker") and
  `opted_in("WYRD_TIER1")`, then calls `run_jepsen_scenario()`
- `run_jepsen_scenario()`: composes the cluster (7 servers), calls
  `run_jepsen_consistency_test()`
- `run_jepsen_consistency_test()`: single call to
  `crate::jepsen::consistency_test_cargo_args()` — the single source of truth

Two in-module tests added to `faults.rs`:
- `jepsen_required_tool_is_docker` — verifies `crate::jepsen::consistency_required_tool()`
  returns `"docker"`
- `jepsen_dispatch_args_target_in_repo_scenario` — verifies
  `crate::jepsen::consistency_test_cargo_args()` names `tier1_jepsen_consistency`

### xtask/tests/jepsen_orchestration.rs (the flippable test)

Imports `use xtask::jepsen::{...}` — all symbols from the lib's `pub mod jepsen`.

**RED mechanism**: `run-verify.sh` classifies `jepsen_orchestration.rs` as an added test
(`xtask/tests/*.rs` matches `*/tests/*.rs`), keeps it in the red step, reverts `lib.rs`
(removing `pub mod jepsen;`) and removes `jepsen.rs` (added non-test file). Result:
`xtask::jepsen` doesn't exist → compilation error → non-zero exit → RED ✓

**GREEN mechanism**: full patch applied → `jepsen.rs` exists, `lib.rs` exports it,
11 non-ignored unit tests pass → GREEN ✓

Confirmed by `run-verify.sh`:
```
run-verify.sh: PASS — red without the fix, green with it.
```

### tier1_jepsen_consistency.rs (the scenario test)

Modelled directly on `tier2_kill_reconstruct.rs` (`tier2_kill_reconstruct.rs:1-795`).
RS(4,2) cluster: 7 servers (6 for initial N=6 placement + 1 spare, domain G).

**Four consistency assertion helpers** (non-`#[ignore]`d, tested at Check):
1. `assert_no_torn_reads` — checks fully-old or fully-new, never hybrid
2. `assert_read_after_commit_from_survivors` — K surviving shards reconstruct original data
3. `assert_no_duplicate_placement_tc` — no duplicate server IDs in committed placement
4. `assert_repair_fired` — spare in committed placement, victim absent, fragment present

Each helper has multiple negative-control unit tests that catch planted anomalies.

**Scenario body** (`#[ignore]`d, `#[tokio::test]`):
- Phase 0: write RS(4,2) fragments to servers 0–5, create inode, enqueue repair
- Phase 1: kill server 0, arm `CrashMeta`, call `reconcile_step` → commit intercepted
  → assert fully-old inode (no torn reads, orphan on spare, read-after-commit from 5
  survivors)
- Phase 2: disarm, call `reconcile_step` → commit lands → assert repair fired, no torn
  reads, no duplicate placement
- Phase 3: read all N post-repair fragments → reconstruct → byte-identical

**Production reach**: `repair::enqueue_repair` is called as a sanctioned test stand-in
for the health-check producer (same precedent as `tier2_kill_reconstruct.rs:545`).

### tier1-jepsen.yml

Nightly 02:00 UTC (before disk-faults at 03:00, integration-nightly at 04:00).
`WYRD_TIER1: "1"`. Docker info check. Artifact upload on failure (same as tier2).

## What I ruled out

### Why not a lib-only module for all of jepsen.rs

If I moved ALL of `jepsen.rs` to lib-only (not `mod jepsen;` in `main.rs`), `faults.rs`
couldn't use `crate::jepsen::consistency_test_cargo_args()`. The only alternative would
be hardcoding the cargo args in `faults.rs` — which breaks the single-source-of-truth
requirement from the carry-forward.

**Cost of hardcoding**: `faults.rs` would have its own `CONSISTENCY_TEST_ARGS = ["test",
"-p", "wyrd-chunkstore-grpc", "--test", "tier1_jepsen_consistency", ...]` (8 items).
Reverting `faults.rs` to the old `WYRD_TIER1_JEPSEN_CMD` shell-out would leave
`xtask::jepsen::consistency_test_cargo_args()` still returning the right values, so the
routing test in `jepsen_orchestration.rs` would still pass — no red. This is exactly the
carry-forward bug from iteration 7.

### Why not split into jepsen_dispatch.rs + jepsen.rs

Alternative: put dispatch items in `jepsen_dispatch.rs` (binary module, no dead-code
issue), put oracle items in `jepsen.rs` (lib-only module). Diff:
- New file `xtask/src/jepsen_dispatch.rs`: ~20 lines
- Refactored `xtask/src/jepsen.rs`: remove dispatch items → ~130 lines remain
- `lib.rs`: `pub mod jepsen; pub mod jepsen_dispatch;`
- `main.rs`: `mod jepsen_dispatch;` (not `mod jepsen;`)
- `faults.rs`: `crate::jepsen_dispatch::...` everywhere
- `jepsen_orchestration.rs`: `use xtask::jepsen_dispatch::...; use xtask::jepsen::...;`
- No `#[allow(dead_code)]` needed

This split removes 4 attribute lines but adds ~100 lines of renamed file + changed
import paths. The conceptual split is also less clear: all these items are part of the
"jepsen consistency harness orchestration". The current approach (4 `#[allow(dead_code)]`
annotations, each explaining why) is smaller and more readable.

## Clippy/format issues caught during build

1. `format!("literal string")` → `.to_string()` in `assert_read_after_commit_from_survivors`
2. Two `for frag_index in 0..N` range loops indexing `initial_placement` → refactored to
   `.iter().enumerate()` per `clippy::needless_range_loop`
3. Several struct-literal style reformats by `cargo fmt` in `jepsen_orchestration.rs`

## Verification

```
$ cargo xtask ci     # GREEN — all checks passed
$ PDCA_BUNDLE=... run-verify.sh
  run-verify.sh: PASS — red without the fix, green with it.
  GREEN: 11 non-ignored tests, 1 ignored; jepsen_orchestration (9 tests); tier1_jepsen_consistency (11 non-ignored)
  RED: xtask::jepsen could not be found → compile error → non-zero exit
```

Path:line citations:
- `faults.rs:141-229` — new `run_jepsen`, `run_jepsen_scenario`, `run_jepsen_consistency_test`
- `faults.rs:367-395` — new `jepsen_required_tool_is_docker`, `jepsen_dispatch_args_target_in_repo_scenario` tests
- `lib.rs:17` — `pub mod jepsen;` added
- `main.rs:28` — `mod jepsen;` added
- `jepsen.rs:1-352` — new file (dispatch items + oracle + unit tests)
- `jepsen_orchestration.rs:1-174` — new integration test (the flippable seam)
- `tier1_jepsen_consistency.rs:1-822` — new scenario test (RS(4,2), 3 phases, 4 helpers + 12 unit tests)
- `tier1-jepsen.yml:1-83` — new privileged CI job
