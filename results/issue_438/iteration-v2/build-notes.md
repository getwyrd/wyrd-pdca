# Build notes — issue 438 / metadata-fdb (iteration 2)

Target: `getwyrd/wyrd` @ `main`, base `182ae4f` (verified `HEAD == origin/main`).
Worktree: `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt`. All `path:line` citations are
against that tree at `182ae4f` + `patch.diff`. Verified: `patch.diff` applies cleanly to a
pristine `182ae4f` checkout.

**This is a re-run.** The design was accepted at sign-off ("the fix is sound — not the
reason for the iterate"). Iteration 1's driver is carried forward essentially unchanged; the
work here is the **three concrete gaps** the sign-off named, plus the one it flagged as
"worth a look". Nothing in the accepted design was re-litigated, and the two things the
sign-off told me *not* to re-raise (C5/T5 production reach; `foundationdb 0.10` adoption) are
left alone.

---

## 1. What changed since `iteration-v1/`

| # | Sign-off gap | Change | Cite |
|---|---|---|---|
| 1 | `deny.toml` — maintainer ruled ISC **allowed**; encode it | `"ISC"` added to the allowlist with an ADR-0003 §2 rationale comment naming `libloading v0.8.9` and its `foundationdb-sys -> (build) bindgen -> clang-sys` path, in the style of the existing `BSD-3-Clause` / `Zlib` entries | `deny.toml:38-45` |
| 2 | The 1021 no-blind-retry rule is defended by no test | The blind path's retry gate is extracted into `blind_commit_step` and the loop into `blind_commit_loop`, driven through a `BlindCommit` seam. Four `store::tests` drive **that production loop** with real `FdbError`s. The named mutation now goes RED. | `crates/metadata-fdb/src/lib.rs:560-651`, `:766-885` |
| 3 | `scan`'s paging loop is dead code under test | New `crates/metadata-fdb/tests/scan.rs`, mirroring `crates/metadata-tikv/tests/scan.rs`: 600 dirents × 512 B ⇒ 5 FDB pages; first-page truncation goes RED | `crates/metadata-fdb/tests/scan.rs:36-134` |
| 4 | *"worth a look"*: `xtask` retried a failing test binary 5× | `run_fdb_test` (5× retry/backoff) replaced by `run_fdb_leg` — **exactly one attempt**, plus a `--lib` leg and the new `scan` leg | `xtask/src/main.rs:403-441` |

Also removed: `contention.rs`'s `commit_unknown_result_is_never_conflict_and_never_retried`.
The sign-off was right that it was vacuous — it never called `commit_blind`, only re-asserted
`classify_commit_error` and grepped a `Display` substring, duplicating
`classify::tests` (`src/lib.rs:158-217`). Deleting a test that *cannot fail for the reason its
name claims* is the point of gap 2, not a weakening: the rule it named is now bound by
`store::tests::a_blind_commit_never_retries_commit_unknown_result` (`src/lib.rs:817`). The
module header records where the rule moved and why (`tests/contention.rs:17-24`).

Zero lines changed in `crates/traits`, `crates/metadata-conformance`, `crates/metadata-tikv`,
`crates/metadata-redb`. The shared suite is driven as-is, **not forked** — the invariant's
self-test.

---

## 2. Gap 2 in detail — why the test had to be shaped this way

The mutation the sign-off named is `blind_commit_step`'s predicate:
`err.is_retryable_not_committed()` → `err.is_retryable()`. Measured against the live client
(`FdbError::from_code(c)`, predicates answered by `libfdb_c` itself):

```
CODE 1020: retryable=true  rnc=true   maybe_committed=false   (not_committed)
CODE 1021: retryable=true  rnc=false  maybe_committed=true    (commit_unknown_result)
CODE 1039: retryable=true  rnc=false  maybe_committed=true    (cluster_version_changed)
CODE 1038: retryable=true  rnc=true   maybe_committed=false   (database_locked)
CODE 2103: retryable=false rnc=false  maybe_committed=false   (value_too_large)
```

So `is_retryable() \ is_retryable_not_committed() = {1021, 1039}` — **exactly** the
maybe-committed set. The mutation is observable *only* if a blind commit is handed a 1021 (or
1039). Everything else is behaviour-identical. That is why iteration 1's tests could not catch
it, and it constrains what a catching test can look like:

- **A 1021 cannot be provoked from a healthy single-node `fdbserver`.** It means "the client
  lost contact after sending the commit". I tried: `TransactionOption::Timeout` yields 1031
  (`retryable=false`, useless); `value_too_large` yields 2103 (`retryable=false`); a blind
  transaction has an empty read-conflict set so the resolver can never reject it with 1020.
  The one deterministic 1021 I *did* find — writing `\xff/conf/locked`, which forces a cluster
  recovery — is unreachable from `commit_blind`, whose transaction never sets
  `ACCESS_SYSTEM_KEYS` and so cannot touch `\xff`.
- Therefore the error must be **injected**, and the thing under test must still be the
  production loop. Hence the seam.

`blind_commit_loop` (`src/lib.rs:607-621`) is the production retry loop: `commit_blind`
(`:536-544`) is now nothing but `blind_commit_loop(&mut TrxBlindCommit { .. })`. The test
supplies a `ScriptedCommit` (`:776-800`) in place of `TrxBlindCommit` — i.e. it replaces the
**error source**, not the loop, not the gate (`blind_commit_step`, `:578-584`), and not the
classification (`outcome_from_commit_error`). The `FdbError`s are real: built by
`FdbError::from_code`, with `is_retryable_not_committed()` answered by `libfdb_c`, not by a
hand-written table that could drift from the client.

Measured RED, this tree (`cargo test -p wyrd-metadata-fdb --features fdb --lib` with the
predicate widened to `is_retryable()`):

```
thread 'store::tests::a_blind_commit_never_retries_commit_unknown_result' panicked at
crates/metadata-fdb/src/lib.rs:814:13:
assertion `left == right` failed: a batch whose commit outcome is unknown must be applied at most once
  left: 2
 right: 1
test result: FAILED. 15 passed; 1 failed;
```

Under the mutation the batch is applied **twice** — the exact double-apply of a non-idempotent
`WriteBatch` that `brief.md:83-86` forbids and that makes `Database::run` unusable here. The
test asserts all three consequences: `attempts == 1`, `resets == 0`, and
`downcast_ref::<CommitUnknownResult>()`.

Three companion tests bound the rest of the arm: `[1020, 1020]` retries and lands
(`attempts == 3`, `resets == 2` — delete the retry arm and this reds); `[2103]` surfaces at
once (`attempts == 1`); `[1020; 5]` exhausts the budget with an "exhausted" error.

### What I rejected here, with the cost

1. **Unit-test only the pure `blind_commit_step`, leaving the loop inline.** ~6 lines instead
   of ~50. Rejected: a mutant can bypass the gate by inlining `if err.is_retryable()` at the
   callsite and the test stays green — precisely the hole the sign-off found. The gate must be
   the *only* predicate in the loop for the test to bind. (It is: `blind_commit_loop` contains
   no `is_retryable*` call.)
2. **Short-circuit 1021 in the loop by classifying first** (`if class == UnknownResult {
   return }` *before* the retry gate). ~4 lines, and it makes the invariant hold structurally.
   Rejected on purpose: it makes the sign-off's named mutation **semantically inert**, so no
   test could ever red it and the next reviewer re-running that mutation would see green and
   conclude the rule is still undefended. Restoring an invariant means the mutation *fails*,
   not that it stops mattering.
3. **Exercise the retry arm end-to-end against the live server with a real
   `database_locked` (1038, retryable-not-committed).** Cost measured, not guessed: it needs
   `\xff/conf/locked` written with `ACCESS_SYSTEM_KEYS` + `LOCK_AWARE` (my probe got 1021 back
   from that write and the DB did *not* end up locked — the special-key route
   `\xff\xff/management/db_locked/<uid>` returned 2115 `no module found`), it locks the whole
   cluster so the other 3 tests in `contention.rs` — which libtest runs on parallel threads —
   would fail spuriously, forcing a 4th test binary (+~90 lines, +1 `xtask` leg), and it
   **still would not red the named mutation** (1038 is retried under both predicates). Two of
   those costs are unbounded flake risk. `a_blind_commit_retries_a_definitively_not_committed_error`
   binds the same arm deterministically, in 12 lines.
4. **`Database::run` closure-retry** — unchanged from iteration 1. Re-runs on 1021.

---

## 3. Gap 3 in detail — the paged scan

`scan_once` (`src/lib.rs:469-493`) follows FDB's `more()` paging. The shared suite's scan
clause stores ≤ 3 keys, which fit in FDB's first page, so `next_range` never fires. Measured
paging thresholds against the live 7.3.77 server (`StreamingMode::WantAll`):

```
count=200  vsize=100  -> pages=1  rows=200
count=600  vsize=500  -> pages=5  rows=600
count=2000 vsize=500  -> pages=14 rows=2000
```

`tests/scan.rs` uses `DIRENTS = 600`, `VALUE_BYTES = 512` (~300 KB — far inside FDB's 10 MB
transaction and 100 KB value limits, so the fixture provokes paging and nothing else).

It does **not** assume the paging: `assert_the_range_really_pages` (`tests/scan.rs:144-170`)
drives the raw client over the same physical range first and asserts the server actually
reports `more()`. This is the same grounding `contention.rs` does for error 1020 — without it,
a future knob change that raised the page budget would silently reduce the completeness
assertion to a single-page read and the test would pass while guarding nothing.

Measured RED (replace `scan_once`'s `next_range` match with `return Ok(out)`):

```
assertion `left == right` failed: paged scan must return the COMPLETE set (600), never a
truncated subset — a scan that stops after its first page lands here
  left: 135
 right: 600
test paged_prefix_scan_returns_the_complete_set_at_scale ... FAILED
```

and in the same run `tests/conformance.rs` stayed **`ok`** — the non-redundancy proof (#419
argument): the shared suite cannot see this class of bug, `scan.rs` is the discriminator.

---

## 4. Gap 4 — the `xtask` retry loop

`run_fdb_test` re-ran a *failing test binary* up to 5×, taking the first success. Inherited
from the TiKV precedent the brief told me to mirror, but for FDB it is both unnecessary and
harmful: it can launder a flaky assertion failure in the sole 1020-pinning test into a green.

Replaced by `run_fdb_leg` (`xtask/src/main.rs:423-441`), one attempt, with the reasoning in the
doc comment: `configure_fdb_database` already polls `status minimal` until the cluster reports
available, and the FDB client **blocks** on a settling cluster (a transaction waits for a read
version) rather than erroring — so a not-yet-ready cluster is a slow first test, not a failure
to retry away. Verified: three cold `docker compose up` → `cargo xtask fdb-conformance` runs,
all four legs green on the first attempt each time.

I did **not** touch `run_tikv_test` (`xtask/src/main.rs:231`). Its retry is load-bearing —
during this cycle's `cargo xtask tikv-conformance` run, `conformance` genuinely failed
attempt 1/5 ("TiKV may still be bootstrapping") and passed on attempt 2. TiKV's client errors
where FDB's blocks. Changing it is out of this slice's scope and would have broken the run.

---

## 5. Forced self-refutation (the three questions)

### (a) Genuine red? — **Yes.** Six independent negations, all re-measured on *this* tree.

"Reverting the fix" on a net-new crate only deletes it, leaving criterion-absence — which
`brief.md:39-40` explicitly forbids resting on. So, per the house demonstrated-red pattern
(`crates/metadata-conformance/tests/demonstrated_red.rs`, #419): negate one production line,
run against the real container, capture the failing assertion.

| ID | Negation | Red witness | New this iteration |
|---|---|---|---|
| **RED-U** | `blind_commit_step`: `is_retryable_not_committed()` → `is_retryable()` | `--lib` `a_blind_commit_never_retries_commit_unknown_result`: applied **twice** | ✅ (gap 2) |
| **RED-P** | `scan_once`: `next_range` match → `return Ok(out)` | `tests/scan.rs`: 135 of 600 keys; `conformance.rs` stays green | ✅ (gap 3) |
| **RED-A** | `commit_conditional`: ignore the observed precondition miss | shared suite: `crates/metadata-conformance/src/lib.rs:74` `left: Committed, right: Conflict` | re-confirmed |
| **RED-B** | `classify_commit_error`: 1020 → `Err` instead of `Conflict` | `contention.rs:173` "writer 1 surfaced a fault instead of a Conflict: Transaction not committed due to conflict with another transaction" | re-confirmed |
| **RED-C** | `classify_commit_error`: drop the `conditional` guard | `contention.rs:291` `left: Conflict, right: Fault`; *and* `--lib` `classify::tests::a_blind_batch_is_never_a_conflict` | re-confirmed |
| **RED-S** | `commit_conditional`: precondition read `snapshot = true` | `contention.rs:176` `exactly one writer must win the race — left: 8, right: 1` (all 8 racers commit: real lost-update corruption); `conformance.rs` stays green | re-confirmed |

RED-A's failing assertion is inside the **shared, unmodified** suite. RED-B's message is FDB's
own text for error 1020, arriving through the **production** `commit`. RED-P and RED-S both
stay green in `conformance.rs`, which is what proves `scan.rs` and `contention.rs` are not
redundant with it.

### (b) Production path? — **Yes.**

- `conformance.rs` drives `wyrd_metadata_conformance::run_all` (the shared runner) against a
  real `FdbMetadataStore` over a real `fdbserver`. Zero lines changed in the suite.
- `contention.rs` and `scan.rs` drive the production `FdbMetadataStore::commit` / `::scan`.
- `store::tests` drive the production `blind_commit_loop` (`src/lib.rs:607`), the production
  `blind_commit_step` (`:578`), and the production `outcome_from_commit_error` — the *same*
  functions `commit_blind` (`:536`) calls, not copies. The only substitution is the
  `BlindCommit` **error source** (`TrxBlindCommit` → `ScriptedCommit`), because the error in
  question is one a healthy cluster cannot emit; the `FdbError`s themselves are real, and
  their retryability is answered by `libfdb_c`.
- Verified linkage: the feature-on test binary links `libfdb_c.so => /lib/libfdb_c.so`; the
  feature-off one links nothing FDB.

### (c) Fixture includes the fault? — **Yes.**

- Real `foundationdb/foundationdb:7.3.77` `fdbserver` (`Database created`, `The database is
  available`), brought up and torn down by `cargo xtask fdb-conformance`.
- The conditional race uses 8 *independent* store handles racing on one seeded key — the
  losing writers are in the fixture, not curated out. RED-S shows they really do lose at the
  resolver.
- The 1020 constant is not asserted from memory: `contention.rs` step 1 provokes a real lost
  race with the raw client and asserts `err.code() == classify::NOT_COMMITTED`.
- The scan fixture asserts the server really reports `more()` before trusting the completeness
  assertion (`tests/scan.rs:144`) — the same grounding move.
- The blind-fault case commits a genuinely oversized value and takes a real server-reported
  `2103 value_too_large`.

### Honest limitation the reviewer should know (carried forward, still true)

`blind_write_race_never_reports_conflict` **cannot fail on FoundationDB.** A write-only FDB
transaction has an empty read-conflict set, so the resolver cannot reject it with 1020; all 8
blind writers legitimately commit. It stayed green under RED-C. It is a *regression guard*,
not a binding test. The binding blind-batch tests are
`blind_batch_commit_error_surfaces_as_err_never_conflict` (red under RED-C) and the four
`store::tests`. I state this rather than let the green tick imply more than it proves.

---

## 6. Carried forward from iteration 1 — the brief defect, restated, not "fixed"

`brief.md:124-125` designates `tests/conformance.rs` the primary witness that must go
red→green **for the 1020 → `Conflict` rule**. It cannot be, and I did not reshape anything to
make it so.

`contract_rename_race_yields_conflict` (`crates/metadata-conformance/src/lib.rs:167-230`) is
**sequential**: the winner's commit completes before the racer's `commit` is called, so the
racer's precondition read observes the winner's write and returns `Ok(Conflict)` from the
**observed-miss** branch. FDB's resolver is never reached; error 1020 never arises;
`classify_commit_error` is never invoked from `conformance.rs`. Under RED-B,
`trait_contract_against_fdb` stays `ok` (re-measured this iteration).

Consequence, handled: `conformance.rs` is bound by **RED-A** (the observed-miss path — a real
failing assertion inside the shared suite), and the 1020 rule is bound by `tests/contention.rs`
(**RED-B**). Between them every clause of the invariant has a demonstrated red against a real
server. Forking or weakening `crates/metadata-conformance` to satisfy the brief's sentence
would violate the very invariant that sentence protects (`brief.md:51-54`).

---

## 7. Dependency wall — the iteration-1 NEEDS-HUMAN is now **closed**

The maintainer ruled at sign-off: **ISC is allowed.** Encoded at `deny.toml:38-45`, naming
`libloading v0.8.9` and its path (`foundationdb-sys -> (build) bindgen -> clang-sys ->
libloading`, which `dlopen()`s libclang to generate the FFI bindings). No ISC-licensed code is
linked into a shipped Wyrd binary. Style mirrors the existing `BSD-3-Clause` and `Zlib`
entries. **Not re-asked.** No `NEEDS-HUMAN external dependency` marker this cycle: all five of
`brief.md`'s external dependencies were met (`libfdb_c.so` 7.3.77 and `fdbcli 7.3 (v7.3.77)`
present on the host, Docker reachable, image pulled, crates.io reachable, `fdb-7_3` matches).
Nothing was stubbed, substituted, or worked around.

Two measured consequences the reviewer should see:

- `cargo deny --all-features check licenses` now reports **`licenses ok`**. It failed on
  pristine `182ae4f` (via `ring`, `Apache-2.0 AND ISC`, behind `tikv-client`). Fixing that
  pre-existing failure was explicitly out of scope; the ISC entry the maintainer ordered
  closes it as a side effect. I added nothing else to the allowlist.
- `cargo deny check` (default features — what `run_ci` invokes, `xtask/src/main.rs:1116`) now
  emits **`warning: unmatched license allowance`** for `"ISC"`, and still exits 0
  (`advisories ok, bans ok, licenses ok, sources ok`; `cargo xtask ci` green). This is the same
  blind spot iteration 1 documented and is *informative, not a regression*: `cargo-deny` audits
  cargo's **feature-resolved** graph, not `Cargo.lock`'s package list, so with `fdb` and `tikv`
  off it sees neither optional backend (`cargo deny list | grep -c foundationdb` → `0`;
  same for `tikv-client`). I did not silence it — a `unused-allowed-license = "allow"` would
  mask real allowlist rot across the whole file. It corrects `brief.md:133-139`'s premise that
  the licence wall gives real Check signal for an optional backend; it does not.

---

## 8. Verification actually performed (all on the final tree)

| Command | Result |
|---|---|
| `cargo xtask fdb-conformance` (× 3, cold container each time) | **green** — `--lib` 16/16, `conformance` 1/1, `contention` 3/3, `scan` 1/1; one attempt per leg |
| `cargo xtask ci` (feature off) | **`xtask ci: all checks passed`** |
| `cargo xtask tikv-conformance` | **green** — TiKV still passes the *same unmodified* suite (the invariant's self-test) |
| `cargo build -p wyrd-metadata-fdb --features fdb` | compiles, links `libfdb_c` |
| `cargo clippy -p wyrd-metadata-fdb --features fdb --all-targets` | clean |
| `cargo fmt --all -- --check` | clean (commit-hook ready) |
| `cargo machete` | no unused deps |
| `cargo deny check` | `advisories ok, bans ok, licenses ok, sources ok` (see §7) |
| `cargo deny --all-features check licenses` | `licenses ok` |
| feature-off `cargo test -p wyrd-metadata-fdb` | 12 unit tests pass; all three binaries skip cleanly |
| `git apply --check patch.diff` on pristine `182ae4f` | applies cleanly |

Passing live output:

```
$ cargo xtask fdb-conformance
$ cargo test -p wyrd-metadata-fdb --features fdb --lib
test store::tests::a_blind_commit_never_retries_commit_unknown_result ... ok
test store::tests::a_blind_commit_retries_a_definitively_not_committed_error ... ok
test store::tests::a_blind_commit_surfaces_a_non_retryable_fault_at_once ... ok
test store::tests::a_blind_commit_gives_up_after_max_attempts ... ok
test result: ok. 16 passed; 0 failed;
$ cargo test -p wyrd-metadata-fdb --features fdb --test conformance
test trait_contract_against_fdb ... ok
$ cargo test -p wyrd-metadata-fdb --features fdb --test contention
test blind_write_race_never_reports_conflict ... ok
test conditional_race_loser_yields_conflict ... ok
test blind_batch_commit_error_surfaces_as_err_never_conflict ... ok
$ cargo test -p wyrd-metadata-fdb --features fdb --test scan
test paged_prefix_scan_returns_the_complete_set_at_scale ... ok
xtask fdb-conformance: FoundationDB passed the shared MetadataStore conformance suite and the contention properties
```

The invariant's self-test (*"keep `redb` **and** `tikv` green in the same PR"*) is satisfied by
**measurement**: redb's conformance runs inside `cargo xtask ci` (green), and
`cargo xtask tikv-conformance` ran against a live TiKV on this exact tree (green, 5/5). The
shared suite has zero changed lines.

---

## 9. What Check cannot prove (for §6 / sign-off)

- **The live legs are off-Check by design** (`brief.md:128-139`). `cargo xtask ci` runs with
  `fdb` off, so it neither links `libfdb_c` nor runs the FDB binaries. The live evidence is §5
  and §8; the human re-confirms with `cargo xtask fdb-conformance` (needs Docker + `libfdb_c`
  7.3.x).
- **`C4-verify` will read green-on-green, not red→green.** It applies `patch.diff` and runs the
  shipped `tests/*.rs` with the feature off and no cluster file, where all three binaries skip
  cleanly by design. That is *not* evidence of a hollow test — the binding red→green is the six
  demonstrated-red runs in §5, each against the real server or the real `libfdb_c` predicate.
  The iteration-1 sign-off correctly flagged that the C4-verify row's evidence string ("no
  pre-patch state to isolate a RED against") is verbatim what `brief.md:39-40` forbids; the row
  should cite the table in §5.
- **Production reach (C5/T5)** — pre-declared at `brief.md:147-156`, **accepted** at the
  iteration-1 sign-off. Nothing in the live path constructs an `FdbMetadataStore`;
  `crates/server/src/cli.rs:133-140` still selects `redb`/`tikv`. Not re-raised.
- **The `foundationdb` dependency-wall judgment tests** (ADR-0003, INTEGRATION §4) remain the
  maintainer's; the licence test now passes mechanically (§7).

### Manual validation steps

```bash
cd <wyrd checkout>
cargo xtask ci                 # feature off; must stay green, must not link libfdb_c
cargo xtask fdb-conformance    # up → --lib, conformance, contention, scan → always tear down
cargo xtask tikv-conformance   # the invariant's self-test: tikv still green on the same suite
cargo deny --all-features check licenses   # now `licenses ok`

# Reproduce the two new demonstrated-reds:
#  RED-U: crates/metadata-fdb/src/lib.rs:579 -> `if err.is_retryable() {`
#         cargo test -p wyrd-metadata-fdb --features fdb --lib   # a_blind_commit_never_retries_… FAILS
#  RED-P: crates/metadata-fdb/src/lib.rs:487 -> replace the `next_range` match with `return Ok(out);`
#         cargo xtask fdb-conformance                            # tests/scan.rs FAILS (135 of 600)
```
