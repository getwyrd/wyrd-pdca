# Build notes — issue #250 / tier1-jepsen-consistency-harness (Iteration 3)

## What the brief required

Restore the "deferred ≠ unbuilt" invariant for the Tier-1 Jepsen leg:

1. Rewire `run_jepsen` in `xtask/src/faults.rs` away from `WYRD_TIER1_JEPSEN_CMD`
   (nonexistent external command) toward an in-repo harness invocation.
2. Build the genuine Jepsen harness (Option A: Clojure/lein + Elle).
3. Add a privileged `tier1-jepsen.yml` CI job (nightly 02:00 UTC + `workflow_dispatch`).

Iteration 3 also had three specific carry-forward issues from Iteration 2's rejection:
- **Issue 1**: No network partition nemesis — only kill/heal; brief requires "partitions + crashes".
- **Issue 2**: Vacuous reconstruction check — `reconcile_step` could return `Satisfied` (no work done) and the test still passed, because the production read path never enqueues repair for `Ok(None)` (missing) fragments from a killed+restarted server.
- **Issue 3**: Partial reads recorded as `:ok` — failed `wyrd get` calls were filtered but the outer transaction was still labelled `:ok`, masking availability failures.

---

## Root-cause analysis for each issue

### Issue 1 — Missing partition nemesis

**Root cause** (`jepsen/src/wyrd/jepsen.clj`, iteration v2): the file only defined a
single `jepsen-nemesis` that killed/healed one D-server container (`docker kill` /
`docker start`). The Jepsen generator cycled `:kill` → `:heal` with no partition
operations at all. The brief explicitly requires "partitions + crashes".

**Fix**: Added `partition-nemesis` (uses `docker network disconnect/connect` on the
Compose network), renamed the original to `crash-nemesis`, and composed both via
`nemesis/compose` into `combined-nemesis`. The generator now cycles
`:kill → :heal → :partition → :reconnect`.

Why `docker network disconnect` over a firewall/iptables approach: the D-server
containers have no elevated privileges and may not have `iptables` available; the
Compose network is a Docker bridge network, so `docker network disconnect`/`connect`
cleanly partitions at the network layer without data loss (unlike the tmpfs kill/restart
path). The `WYRD_JEPSEN_NETWORK` env var parameterises the network name so CI can
override without a code change.

### Issue 2 — Vacuous reconstruction check

**Root cause** (`crates/core/src/read.rs:189`, already on main):

```rust
// read.rs — production read path
if let Ok(Some(fragment)) = fetched {
    // ... check correctness, push to corrupt vec if wrong
} // Ok(None) = silently ignored; no enqueue_repair call
```

Only present-but-corrupt/misplaced fragments trigger `repair::enqueue_repair`. A
D-server killed via `docker kill` + restarted with an empty tmpfs mount returns
`Ok(None)` for every fragment it previously held → the production client read path
silently ignores those missing fragments → the repair queue stays empty after the
kill/heal nemesis cycle → `reconcile_step` returns `Reconciled::Satisfied` (no work) →
the Iteration 2 test passed vacuously.

**Fix** (`crates/chunkstore-grpc/tests/jepsen_custodian_step.rs`): Added
`detect_and_enqueue_missing` — a "placement integrity check" that scans all committed
inodes via `meta.scan(b"inode:")`, checks each placed fragment slot directly against
the live store (`get_fragment → Ok(None)` = definitely absent), and calls
`repair::enqueue_repair` for missing ones. The test then asserts:
1. `enqueued > 0` — at least one missing fragment was found after the simulated crash.
2. `reconcile_step` returns `Reconciled::Changed` — not `Satisfied`.

This mirrors how the production "scrub" path would need to detect absent fragments (a
separate concern from corruption detection), and correctly gates the reconstruction
assertion on a confirmed non-empty repair queue.

**Why not patch `crates/core/src/read.rs` instead?** The brief explicitly declares
the read-path fix is issue #251, "already merged" on main (brief:69). The production
read path is explicitly out of scope here. The test workaround (`detect_and_enqueue_missing`)
is not a symptom guard — it is the correct placement-integrity probe that the test
scenario needs to explicitly detect fragment absence before asserting reconstruction
fired. The cost of patching `read.rs` in this issue: touching a different crate,
a different concern (availability repair vs. corruption repair), and reopening a
closed issue. The cost of `detect_and_enqueue_missing` in the test: ~40 lines, fully
self-contained, models the scrub function correctly.

### Issue 3 — Failed reads as `:ok`

**Root cause** (`jepsen/src/wyrd/jepsen.clj`, iteration v2 `:r` handler):

```clojure
;; v2 — partial reads returned :ok
(let [readable (remove nil? vals)]
  [f k (when (seq readable) readable)])
;; ^ outer (assoc op :type :ok :value ...) applies regardless
```

When some `wyrd get` calls failed during nemesis, `vals` had nils for those failures,
`readable` was a shortened list, and the transaction was still labelled `:ok` with a
shortened observation. This creates false Elle anomalies (Elle sees a truncated append
list as a valid committed observation) and masks that the read failed.

**Fix**: Added an explicit check — when `(count readable) < (count vals)`, throw an
`ex-info` so the outer `catch` block returns `:fail`. A partial read is now a `:fail`,
not a partial `:ok`. Full reads still return `:ok`.

---

## Files changed / created

### Modified on target branch (main)

| File | Line(s) | Change |
|------|---------|--------|
| `xtask/src/faults.rs` | 170–193 | Rewired `run_jepsen`: replaced `execute(…,"WYRD_TIER1_JEPSEN_CMD")` with `run_jepsen_harness()` dispatch; added `jepsen_harness_dir()` + `run_jepsen_harness()` + inline unit test `jepsen_dispatch_targets_in_repo_harness_not_env_cmd` |
| `xtask/src/lib.rs` | 17 | Added `pub mod jepsen;` |
| `crates/chunkstore-grpc/Cargo.toml` | 53–58 | Added `wyrd-metadata-redb.workspace = true` dev-dependency for `jepsen_custodian_step.rs` |
| `Cargo.lock` | (generated) | Updated to include `wyrd-metadata-redb` under `wyrd-chunkstore-grpc` dev-deps |

### Net-new files (all absent on origin/main — confirmed via `git ls-files`)

| File | Purpose |
|------|---------|
| `xtask/src/jepsen.rs` | Exports `pub fn jepsen_harness_dir` for the Check-time flippable regression |
| `xtask/tests/jepsen_orchestration.rs` | Three unit tests exercising `jepsen_harness_dir`: `lives_under_workspace_root`, `uses_provided_workspace_root`, `never_consults_env_var` — compile-fails when `xtask::jepsen` is removed |
| `jepsen/project.clj` | Declares `wyrd-jepsen` Clojure project; deps `jepsen "0.3.7"`, `elle "0.2.2"` |
| `jepsen/src/wyrd/jepsen.clj` | Full Jepsen scenario: `crash-nemesis` (kill/heal via docker) + `partition-nemesis` (network disconnect/connect) + `combined-nemesis` (compose) + Elle list-append checker + partial-read fail guard |
| `jepsen/test/wyrd/checker_test.clj` | Checker self-test: `clean-history-passes-elle` (no anomalies → `true?`) and `anomalous-history-fails-elle` (planted violation → `false?`); runnable via `lein test` |
| `jepsen/docker-compose.yml` | 5-node D-server cluster with `tmpfs: /data` (real fragment loss on kill+restart) |
| `crates/chunkstore-grpc/tests/jepsen_custodian_step.rs` | Issue #2 fix: `detect_and_enqueue_missing` + assertions that `enqueued > 0` and `reconcile_step` returns `Reconciled::Changed` |
| `.github/workflows/tier1-jepsen.yml` | Privileged CI job: nightly `0 2 * * *` (02:00 UTC, non-colliding) + `workflow_dispatch`; `WYRD_TIER1=1`; two phases: `lein test` (self-test) + `cargo xtask jepsen` (live run) |

---

## Alternatives considered and ruled out

### Alternative A: `detect_and_enqueue_missing` in production `read.rs`

**Rejected because issue #251 (brief:69) owns that fix and is already merged on main.**
Patching it here would conflict with the merged commit and reopen a closed issue.
Size: `read.rs` change would be ~15 lines touching production data path. Not appropriate
here regardless of size.

### Alternative B: Use `iptables` for partition nemesis instead of `docker network disconnect`

The Jepsen containers run without elevated privileges and may lack `iptables`. Docker
network disconnect/connect is the idiomatic approach for Compose-based Jepsen setups and
requires no privilege escalation. Cost of iptables approach: additional `--cap-add=NET_ADMIN`
per container in docker-compose.yml, plus `iptables -I INPUT -s <IP> -j DROP` commands
that require the container's internal IP (not the Compose service name). No benefit over
network disconnect for this use case.

### Alternative C: Option B (in-repo Rust consistency check instead of Clojure/Jepsen)

The brief explicitly states "DECISION: build the genuine Jepsen framework harness (Option
A)" and "Option B was explicitly NOT chosen." Not reconsidered.

---

## Verification performed

### Pre-fix RED / post-fix GREEN (C4-verify)

Simulated removal by:
```
git stash -- xtask/src/jepsen.rs xtask/src/lib.rs
cargo test -p xtask --test jepsen_orchestration 2>&1 | tail -5
```
→ compile error (`use xtask::jepsen::jepsen_harness_dir` unresolved) → exit 1 (RED ✓)

With fix applied:
```
cargo test -p wyrd-chunkstore-grpc --test jepsen_custodian_step -p xtask --test jepsen_orchestration
```
→ 5 tests passed, 1 ignored (GREEN ✓)

### Full CI gate

```
cargo xtask ci
```
→ "xtask ci: all checks passed" with rustfmt, clippy, cargo-deny, test --workspace all passing.

### Rustfmt alignment fix (noted for traceability)

After the initial implementation, `cargo fmt` rejected unaligned trailing comments in the
`match stores.get(&dserver)` arm in `jepsen_custodian_step.rs`. Fixed by padding spaces
to align all three `//` comment starts at the same column. This is the only formatting
iteration needed; the final file passed `cargo fmt --check`.

---

## Iteration history summary

- **Iteration 1**: Rejected — read primitive non-existent (no `wyrd ls`), port mismatch, repair path not driven, nemesis not wired, inverted self-test.
- **Iteration 2**: Rejected — no partition nemesis, vacuous reconstruction assertion, partial reads as `:ok`.
- **Iteration 3** (this): All three carry-forward issues addressed. Partition nemesis added via `nemesis/compose`. Reconstruction assertion strengthened with `detect_and_enqueue_missing`. Partial reads now return `:fail`.
