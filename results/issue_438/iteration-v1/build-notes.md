# Build notes — issue 438 / metadata-fdb

Target: `getwyrd/wyrd` @ `main`, base `182ae4f` (verified `HEAD == origin/main`).
Worktree: `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt`. All `path:line` citations below
are against that tree at `182ae4f` + this patch. `patch.diff` verified to apply cleanly to a
pristine `182ae4f` checkout.

---

## 1. What was built

A new workspace crate `crates/metadata-fdb` implementing the **unchanged** `MetadataStore`
trait (`crates/traits/src/lib.rs:338`) over the `foundationdb` crate, feature-gated `fdb`,
off by default. Zero lines changed in `crates/traits`, `crates/metadata-conformance`,
`crates/metadata-tikv`, `crates/metadata-redb`, or `deny.toml` — this is a composition
slice (ADR-0010), and the shared suite is driven **as-is**, not forked.

| File | Role |
|---|---|
| `crates/metadata-fdb/Cargo.toml` | `default = []`, `fdb = [...]`, all backend deps `optional`, `cargo-machete` ignores — mirrors `crates/metadata-tikv/Cargo.toml:11-51` |
| `crates/metadata-fdb/src/lib.rs:66-213` | `classify` — the load-bearing commit-error rule (`classify_commit_error` at `:112`), **dependency-free**, unit-tested feature-off |
| `crates/metadata-fdb/src/lib.rs:215-262` | `config` — `WYRD_FDB_CLUSTER_FILE` / `/etc/foundationdb/fdb.cluster`, pure fn, owned by the driver's constructor |
| `crates/metadata-fdb/src/lib.rs:264-353` | `keyspace` — per-instance prefix math, dependency-free |
| `crates/metadata-fdb/src/lib.rs:355-648` | `store::FdbMetadataStore` — the live driver (`#[cfg(feature = "fdb")]`); `ensure_network` at `:391`, the single classification site `outcome_from_commit_error` at `:567` |
| `crates/metadata-fdb/tests/conformance.rs` | shared `run_all`, all **7** clauses, cluster-file-gated clean skip |
| `crates/metadata-fdb/tests/contention.rs` | 1020→`Conflict` and blind-batch-`Err` rules |
| `deploy/fdb-single-node/docker-compose.yml` | throwaway single-node `fdbserver`, outside the Cargo workspace (ADR-0010) |
| `xtask/src/main.rs` (`run_fdb_conformance` et al.) | compose-up → configure → wait → run → **always** tear down, mirroring `run_tikv_conformance` |

### Design decisions worth the reviewer's attention

**The `conditional` guard is the whole invariant.** `classify_commit_error(code, conditional)`
(`src/lib.rs:112`) is the *single* classification rule; both `commit_conditional` (`:491`) and
`commit_blind` (`:524`) route through `outcome_from_commit_error` (`:567`). Order is load-bearing: 1021 is
checked **before** 1020, so `commit_unknown_result` is never `Conflict` for *either* batch
shape. This reproduces `crates/metadata-tikv/src/lib.rs:542-546`.

**`snapshot = false` on the precondition read is the mechanism, not a detail.** A
non-snapshot read joins the FDB read-conflict set; that is what makes a lost race surface as
1020 instead of a silent last-writer-wins overwrite. Proven load-bearing by RED-S below (a
snapshot read makes all 8 racers commit — a real lost-update corruption).

**One network thread per process.** `foundationdb::boot()` is booted once behind a
`static OnceLock<NetworkAutoStop>` (`src/lib.rs`, `ensure_network`). `run_all` builds 7 stores
and `contention.rs` builds 10 in one process each; both pass. The guard is deliberately never
dropped (a `static` is not dropped at exit, and stopping the run loop while a `Database` lives
is UB).

**`Database::run` is not used** — the brief names the hazard and it is real: its closure-retry
re-runs on 1021, and a `WriteBatch` is not guaranteed idempotent. The blind path retries only
on `is_retryable_not_committed()`, a predicate that is *false* for 1021 by definition, so 1021
structurally cannot enter the retry arm. A conditional batch is never driver-retried at all —
the caller owns that (`crates/server/src/cli.rs:1027-1049`).

---

## 2. Forced self-refutation (the three questions)

### (a) Genuine red? — **Yes**, four independent negations, each a real failing assertion against a live `fdbserver`.

"Reverting the fix" on a net-new crate would only delete it, leaving criterion-absence — which
the brief explicitly forbids resting on. So I used the house demonstrated-red pattern
(`crates/metadata-conformance/tests/demonstrated_red.rs`, #419): negate one production line,
run `cargo xtask fdb-conformance` against the real container, capture the failure.

**RED-A — ignore the observed precondition miss** (`if !holds { return Ok(Conflict) }` → proceed).
Reds the brief's *primary* test file:
```
thread 'trait_contract_against_fdb' panicked at crates/metadata-conformance/src/lib.rs:74:5:
assertion `left == right` failed
  left: Committed
 right: Conflict
test trait_contract_against_fdb ... FAILED
test result: FAILED. 0 passed; 1 failed;
```
The failing assertion is inside the **shared, unmodified** suite — not a local copy.

**RED-B — negate the 1020 → `Conflict` classification** (return `Err` instead):
```
thread 'conditional_race_loser_yields_conflict' panicked at crates/metadata-fdb/tests/contention.rs:175:27:
writer 1 surfaced a fault instead of a Conflict: Transaction not committed due to conflict with another transaction
test conditional_race_loser_yields_conflict ... FAILED
```
The message is FDB's own text for error 1020, arriving through the **production** `commit`.
This proves the losers genuinely traverse the *lost-race* (1020) path, not the cheaper
observed-miss path — the single most important thing to establish about this driver.

**RED-C — drop the `conditional` guard** (a blind batch's 1020 becomes `Conflict`):
```
thread 'blind_batch_commit_error_surfaces_as_err_never_conflict' panicked at contention.rs:294:9:
assertion `left == right` failed: a BLIND batch is never Conflict: ...
  left: Conflict
 right: Fault
test blind_batch_commit_error_surfaces_as_err_never_conflict ... FAILED
```
It *also* reds a feature-off unit test that runs inside `cargo xtask ci` on every machine:
```
test classify::tests::a_blind_batch_is_never_a_conflict ... FAILED
```

**RED-S — make the precondition read a snapshot read** (`snapshot = true`):
```
thread 'conditional_race_loser_yields_conflict' panicked at contention.rs:178:9:
assertion `left == right` failed: exactly one writer must win the race
  left: 8
 right: 1
```
All 8 racers commit — a genuine lost-update corruption. `conformance.rs` stays **green**
through it, which is the non-redundancy proof: the sequential suite cannot see this class of
bug, and `contention.rs` is the discriminator (the #419 argument).

### (b) Production path? — **Yes.**
`conformance.rs` drives `wyrd_metadata_conformance::run_all` (the shared runner,
`metadata-conformance/src/lib.rs:291`) against a real `FdbMetadataStore` over a real
`fdbserver`. `contention.rs` drives the production `FdbMetadataStore::commit`. The classifier
assertions call `classify::classify_commit_error` — the *exact* function `commit` calls on
both its paths (`src/lib.rs`, `outcome_from_commit_error`), not a copy. No mock, no
re-implementation, no double. Verified linkage: the feature-on test binary links
`libfdb_c.so => /lib/libfdb_c.so`; the feature-off one links nothing FDB.

### (c) Fixture includes the fault? — **Yes.**
The cluster is a real `foundationdb/foundationdb:7.3.77` `fdbserver` (`Database created`,
`The database is available.`), not a fake. The conditional race uses 8 *independent* store
handles racing on one seeded key — the losing writers are in the fixture, not curated out.
The blind-fault case commits a genuinely oversized value and takes a real server-reported
`2103 value_too_large`. The 1020 constant is not asserted from memory: `contention.rs`
step 1 provokes a real lost race with the raw client and asserts `err.code() ==
classify::NOT_COMMITTED` — i.e. the constant is grounded in what *this server* actually emits
before the classifier is pinned on it.

### Honest limitation the reviewer should know
`blind_write_race_never_reports_conflict` **cannot fail on FoundationDB.** A write-only FDB
transaction has an empty read-conflict set, so the resolver cannot reject it with 1020 at all;
all 8 blind writers legitimately commit. It stayed green under RED-C. It is a *regression
guard*, not a binding test. The blind-batch clause that actually binds is
`blind_batch_commit_error_surfaces_as_err_never_conflict` (red under RED-C, above), plus the
feature-off unit test. I state this rather than let the green tick imply more than it proves.

---

## 3. Deviation from the brief's Falsifiability clause (brief defect, not a shortfall)

The brief asserts: *"deliberately negate the 1020→`Conflict` classification (return `Err`
instead) and `contract_rename_race_yields_conflict` **fails**."*

**Measured: it does not.** Under RED-B, `trait_contract_against_fdb` (which runs
`contract_rename_race_yields_conflict`) stayed `ok`; only `contention.rs` went red.

Why: `contract_rename_race_yields_conflict`
(`crates/metadata-conformance/src/lib.rs:167-230`) is **sequential** — the winner's commit
completes *before* the racer's `commit` is called. The racer's precondition read therefore
takes a fresh read version, observes the winner's write, and fails the byte-compare. It
returns `Ok(Conflict)` via the **observed-miss** path and never reaches FDB's resolver, so no
1020 is ever produced. No conflict-classification code runs.

Consequence, handled: the 1020 rule is bound by `tests/contention.rs`
(RED-B), and `tests/conformance.rs` is bound by RED-A (the observed-miss path). Both are real
failing assertions against a real server; between them every clause of the invariant has a
demonstrated red. Nothing is left resting on "the crate did not exist yet." I did **not**
weaken or reshape the shared suite to make the brief's sentence come true — that would have
violated the invariant it was protecting.

---

## 4. Dependency wall — a finding that corrects the brief

The brief predicted: *"`cargo deny check` (which `run_ci` invokes) **does** see the new
`foundationdb` dependency tree, because an optional dep lands in `Cargo.lock` even with its
feature off ... So a disallowed transitive licence fails **at Check**, feature-off. This is
real Check signal, not scaffolding."*

**Measured: false.** `cargo-deny` audits cargo's *feature-resolved* graph, not `Cargo.lock`'s
literal package list. On this tree:

```
$ cargo deny list | grep -ci foundationdb     ->  0
$ cargo deny list | grep -ci tikv-client      ->  0     # pre-existing: same blind spot
```

So the default gate sees **neither** optional backend. The premise is wrong for TiKV today,
not merely for FDB. `cargo deny check` is green on my tree *because it never looks at the
tree*, not because the tree is clean.

And the tree is **not** clean. Under `--all-features`:

```
error[rejected]: failed to satisfy license requirements
 ├ ISC - ISC License:
 ├ libloading v0.8.9
   └── clang-sys v1.8.1
       └── bindgen v0.72.1
           └── (build) foundationdb-sys v0.10.0
               └── foundationdb v0.10.0
                   └── wyrd-metadata-fdb v0.0.0
```

`ISC` is **not** on `deny.toml`'s allowlist (`deny.toml:25-38`). The brief's claim that "the
*licence* test should pass mechanically" holds for `foundationdb` itself (`MIT/Apache-2.0`)
but not for its transitive **build-dependency** tree.

Per the brief — *"Any transitive licence not on the allowlist is a **stop-and-declare**, not a
`deny.toml` edit Do makes unilaterally"* — I did **not** touch `deny.toml`.

Scoping facts for the maintainer, all measured:
- I introduce **no regression** to the gate that runs. `cargo deny check` is green on
  `182ae4f` and green on `182ae4f + this patch`.
- `cargo deny --all-features check licenses` **already FAILED on pristine `182ae4f`**
  (`ring v0.17.14`, `Apache-2.0 AND ISC`, via `tikv-client`). I add a second ISC row
  (`libloading`); I do not create the failure mode.
- `libloading` is a **build-time** dependency (bindgen generates the FFI bindings); no ISC
  code is linked into a shipped Wyrd binary. ISC is permissive, OSI-approved and
  GPL-compatible.
- `embedded-fdb-include` does *not* remove this: `bindgen` is an unconditional build-dep of
  `foundationdb-sys`. That feature removes the *header* requirement only.
- Full set of 20 crates newly entering `Cargo.lock`, with licences:
  `foundationdb{,-sys,-gen,-tuple,-macros}` MIT/Apache-2.0 · `async-recursion` MIT OR
  Apache-2.0 · `bindgen` BSD-3-Clause · `cexpr` Apache-2.0/MIT · `clang-sys` Apache-2.0 ·
  `glob` MIT OR Apache-2.0 · **`libloading` ISC** · `minimal-lexical` MIT/Apache-2.0 · `nom`
  MIT · `prettyplease` MIT OR Apache-2.0 · `rustc-hash` Apache-2.0 OR MIT · `serde_bytes` MIT
  OR Apache-2.0 · `shlex` MIT OR Apache-2.0 · `static_assertions` MIT OR Apache-2.0 ·
  `try_map` Apache-2.0/MIT · `xml`, `xml-rs` MIT.

Everything except `libloading` is already on the allowlist.

```
NEEDS-HUMAN external dependency: libloading v0.8.9 (ISC), a transitive build-dependency of foundationdb-sys via bindgen -> clang-sys — ISC is not on deny.toml's allowlist (deny.toml:25-38). No PDCA gate covers it: `cargo deny check` as run by `run_ci` resolves default features and therefore sees neither `foundationdb` nor `tikv-client` (verified: `cargo deny list | grep -c foundationdb` -> 0). I could not produce licence-wall clearance for the fdb tree, and per the brief I did not edit deny.toml unilaterally. Maintainer call at sign-off (ADR-0003 three-test audit): either allow "ISC" (already required by `ring` behind the pre-existing tikv-client tree, so `cargo deny --all-features check licenses` already fails on pristine main) or reject the foundationdb build-dep tree.
```

---

## 5. Alternatives considered and rejected (with costs)

1. **`Database::run` closure-retry.** Rejected: re-runs the closure on 1021
   `commit_unknown_result`, double-applying a non-idempotent `WriteBatch`. The brief names
   this hazard; it is real (`foundationdb-0.10.0/src/database.rs`, the retry loop calls
   `on_error`, which treats 1021 as retryable).
2. **Share `keyspace` with `metadata-tikv` instead of duplicating 3 pure fns (~30 lines).**
   Rejected on the dependency rule, not on size: ADR-0010 forbids a concrete backend
   depending on a sibling concrete. Doing it properly means a new `crates/metadata-keyspace`
   crate: +1 crate dir (Cargo.toml + lib.rs), +2 lines in the root `Cargo.toml` (member +
   workspace dep), and **moving 81 lines out of `crates/metadata-tikv/src/lib.rs:31-111`** —
   i.e. editing the very crate whose green the invariant's self-test says this PR must not
   destabilise. Concretely: 5 files touched, ~120 lines moved, vs 30 lines duplicated. Deferred
   as its own refactor; noted in the module doc.
3. **Provoke a live blind-batch 1020 to bind the blind rule by race.** *Physically impossible
   on FoundationDB*: a write-only transaction has an empty read-conflict set, so the resolver
   cannot reject it. Manufacturing one would require the driver to `add_conflict_range` a read
   range it never read — i.e. inventing the very failure the invariant forbids. Instead the
   blind rule is bound by driving the production classifier with a **real 1020 obtained from
   the live server** (`contention.rs` step 1 → step 2) plus a real blind commit fault
   (`2103`, step 3). RED-C confirms this binds.
4. **`#![forbid(unsafe_code)]`** (as `metadata-tikv` does). Impossible: `foundationdb::boot()`
   is an `unsafe fn` and `api::NetworkBuilder::boot` is `unsafe` too — there is no safe entry
   point. Used `#![deny(unsafe_code)]` plus exactly one `#[allow(unsafe_code)]` block with a
   `SAFETY:` comment discharging both documented obligations.
5. **Boot the FDB network per store.** Rejected: the client permits one network thread per
   process and `run_all` builds seven stores. `OnceLock` gives exactly-once init even under
   concurrent `make_store`.
6. **Bind-mount the cluster file from the container into the repo.** Rejected: writes into the
   source tree and needs container-uid-dependent permissions. `xtask` writes a byte-identical
   host-side file under `target/` (already git-ignored) matching the compose file's pinned
   `FDB_CLUSTER_FILE_CONTENTS`.

---

## 6. Verification actually performed (all on the final tree)

| Command | Result |
|---|---|
| `cargo build -p wyrd-metadata-fdb --features fdb` | compiles, links `libfdb_c` |
| `cargo clippy -p wyrd-metadata-fdb --features fdb --all-targets` | clean |
| `cargo xtask fdb-conformance` | **green** — `conformance` 1/1, `contention` 4/4 vs live `fdbserver` |
| `cargo xtask ci` (feature off) | **`xtask ci: all checks passed`** |
| `cargo xtask tikv-conformance` | **green** — TiKV still passes the *same unmodified* suite |
| `cargo fmt --all -- --check` | clean (commit-hook ready) |
| `cargo machete` | no unused deps |
| `cargo deny check` | `advisories ok, bans ok, licenses ok, sources ok` (see §4 caveat) |
| feature-off `cargo test -p wyrd-metadata-fdb` | 12 unit tests pass; both binaries skip cleanly |
| `ldd` feature-off test binary | **no `libfdb_c`** |
| `git apply --check patch.diff` on pristine `182ae4f` | applies cleanly |

Passing live output:
```
$ cargo xtask fdb-conformance
Database created
test trait_contract_against_fdb ... ok
test result: ok. 1 passed; 0 failed; ...
test commit_unknown_result_is_never_conflict_and_never_retried ... ok
test blind_write_race_never_reports_conflict ... ok
test conditional_race_loser_yields_conflict ... ok
test blind_batch_commit_error_surfaces_as_err_never_conflict ... ok
test result: ok. 4 passed; 0 failed; ...
xtask fdb-conformance: FoundationDB passed the shared MetadataStore conformance suite and the contention properties
```

The invariant's self-test (*"keep `redb` **and** `tikv` green in the same PR"*) is satisfied by
**measurement, not assertion**: redb's conformance runs inside `cargo xtask ci` (green), and I
ran `cargo xtask tikv-conformance` against a live TiKV on this exact tree (green, 5/5). The
shared suite has zero changed lines.

### Environment note (resolves a Plan-time blocker)
The brief recorded `libfdb_c` as **"Currently ABSENT on this host"** and instructed me to stop
and declare if so. It is now **present** and version-correct — `libfdb_c.so 7.3.77`,
`fdbcli 7.3 (v7.3.77)`, matching the `foundationdb/foundationdb:7.3.77` server and the
`fdb-7_3` api feature. Docker reachable, image pulled. So external dependencies 1–5 were all
met; nothing was worked around, stubbed, or substituted.

---

## 7. What Check cannot prove (for §6 / sign-off)

- **The live legs are off-Check by design.** `cargo xtask ci` runs with `fdb` off, so it
  neither links `libfdb_c` nor runs the two FDB binaries. The live evidence is §6 above; the
  human re-confirms with `cargo xtask fdb-conformance` (needs Docker + `libfdb_c` 7.3.x).
- **`C4-verify` will read green-on-green, not red→green.** It applies `patch.diff` and runs
  the shipped `tests/*.rs` with the feature off and no cluster file, where both binaries skip
  cleanly by design. That is *not* evidence of a hollow test — the binding red→green is the
  four demonstrated-red runs in §2, each against the real server.
- **Production reach (C5/T5), pre-declared in the brief.** Nothing in the live path constructs
  an `FdbMetadataStore`: `crates/server/src/cli.rs:133-140` still selects `redb`/`tikv`, and
  the FDB selection arm is an explicitly-blocked later issue. The seam is honoured only by the
  two test binaries — but load-bearingly, against a real server over the shared suite.
- **The `libloading`/ISC licence call (§4)** — the one genuine NEEDS-HUMAN this cycle.

### Manual validation steps
```bash
cd <wyrd checkout>
cargo xtask ci                 # feature off; must stay green, must not link libfdb_c
cargo xtask fdb-conformance    # brings up fdbserver, runs both binaries, always tears down
cargo xtask tikv-conformance   # the invariant's self-test: tikv still green on the same suite
cargo deny --all-features check licenses   # will FAIL: ring(ISC, pre-existing) + libloading(ISC, new)
```
