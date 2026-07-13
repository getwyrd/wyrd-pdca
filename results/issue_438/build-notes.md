# Build notes — issue 438 / metadata-fdb (iteration 3)

Target branch: `getwyrd/wyrd @ main`, tip `182ae4f` (verified: `patch.diff` applies clean to
`182ae4f` in a scratch checkout). All `path:line` citations below are against the post-patch
tree unless prefixed `origin/main`.

Environment (all four external dependencies the brief enumerated are **present**; no
NEEDS-HUMAN external-dependency declaration is required this cycle):

- `libfdb_c` 7.3 on the build host: `/lib/libfdb_c.so` (`ldconfig -p`), `fdbcli` at
  `/usr/bin/fdbcli`.
- Docker 29.6.1 reachable; `foundationdb/foundationdb:7.3.77` pulled.
- Live single-node `fdbserver` brought up by `cargo xtask fdb-conformance` for every run
  below (`deploy/fdb-single-node/docker-compose.yml`), torn down after each.
- `foundationdb = "0.10"`, `default-features = false`, `features = ["fdb-7_3"]`.

Runner used throughout: the project's own `./engine/xtask.sh` wrapper (→ `cargo xtask …`
inside `$PDCA_WORKTREE`). No hand-rolled test invocation.

---

## 1. What this iteration changes, and why

Iteration 2's design was accepted; the sign-off named three concrete, in-scope gaps plus
four low-severity items. This iteration closes all seven. I did **not** re-attempt anything
that was rejected, and I did not re-open the design, the seam, the `foundationdb 0.10` pin,
the `deny.toml` ISC entry, or C5/T5.

### Gap 1 (THE BLOCKER) — `scan` had no `SCAN_CAP`

The sign-off's instruction was exact: *"Mirror the TiKV peer: same 2^20 constant, same
`ScanCapExceeded` error type, no partial results, plus a test."* Done, as a new
dependency-free `paging` module so it compiles and is unit-tested on **every** machine:

- `crates/metadata-fdb/src/lib.rs:441` — `pub const SCAN_CAP: usize = 1 << 20`, the sibling
  backend's constant verbatim (`crates/metadata-tikv/src/lib.rs:145`). There is a unit test
  (`the_cap_matches_the_sibling_backend`) whose only job is to fail if the two ever drift:
  two backends of one trait must not disagree about how large a listing may be.
- `crates/metadata-fdb/src/lib.rs:449` — `pub struct ScanCapExceeded { cap, prefix }`, same
  shape and same `Display` wording as `crates/metadata-tikv/src/lib.rs:155`.
- `crates/metadata-fdb/src/lib.rs:490` — `pub fn after_page(total, cap) -> PageStep`, the
  decision function. `total > cap` (not `>=`), matching the peer's boundary at
  `crates/metadata-tikv/src/lib.rs:217`, so a scan returning exactly `cap` keys is a legal
  complete result.
- `crates/metadata-fdb/src/lib.rs:755` — enforced in `scan_once`, **after each page and
  before FDB's `next_range` cursor is consulted**, mirroring the peer's "cap is checked
  first" ordering: an over-cap set can never slip through as a "complete" final page.
- `crates/metadata-fdb/src/lib.rs:1130` — `scan` turns the breach into
  `Err(ScanCapExceeded)` and returns **no partial `Vec`**. Not retried: the breach is
  deterministic in the data, so the four extra attempts the old loop would have spent are
  pure waste.

FDB does its own page-cursor arithmetic (`RangeOption::next_range`), so unlike TiKV this
module carries no `PAGE_SIZE` / `next_page_start`. The only decision left to the driver is
the one the peer flags as a **correctness constraint, not a tuning knob** — the ceiling —
and that is exactly what I lifted.

**The test seam, and why it is not a cheat.** `with_scan_cap`
(`crates/metadata-fdb/src/lib.rs:712`) **lowers** the cap; values above `SCAN_CAP` are
clamped back to it, so a caller cannot loosen a correctness constraint into a knob. This
exists because the fail-loud arm is otherwise unreachable by any honest test: proving it at
the real 2^20 would mean writing a million keys per run, and FDB's 5 s / 10 MB transaction
envelope would trip `1007 transaction_too_old` first — the test would witness the envelope,
never the cap. With the cap lowered, `tests/scan.rs:74` drives the **production** `scan`,
the **production** paging loop, the **production** `after_page`, and the **production**
`ScanCapExceeded`. Only the number is scripted.

I considered and rejected: (a) unit-testing `after_page` alone, as the peer does — that is
precisely the vacuous shape the iteration-1 and iteration-2 sign-offs rejected twice, since
nothing would then bind the *wiring* into `scan_once`; RED-C1 below shows the wiring
mutation survives a pure-`after_page` test suite. (b) A `#[cfg(test)]`-only cap — it would
not exist in the binary the live test runs, so the live test could not reach it.

### Gap 2 — retry-exhaustion destroyed the downcast contract

`crates/metadata-fdb/src/lib.rs:585` adds `RetryBudgetExhausted { op, attempts, last:
FdbError }` with `source()` returning the `FdbError` (`:610`). The three exhaustion sites
(`blind_commit_loop`, `get`, `scan`) now return it instead of
`BoxError::from(format!("… exhausted 5 attempts …"))`.

The crate's own module doc promised a caller could tell a transient `1007
transaction_too_old` from a permanent `2103 value_too_large` by downcasting. A String-backed
error returns `None` from `downcast_ref::<FdbError>()` — the crate contradicted itself on
exactly the paths where the cause matters most (a blind commit failing 5× with 1007 takes
the Retry arm every time and lands there). I also corrected the doc so it now describes the
real shape: the cause is reachable via `source()`, not by downcasting the outer error
(`crates/metadata-fdb/src/lib.rs`, "Errors a caller can tell apart").

### Gap 3 — the module doc overclaimed; two mutants survived

**(a) The doc.** The old text said the `conditional` guard "is the whole of the invariant's
third clause" and that it "is pinned by `tests/contention.rs`". Both false, and the reviewer
proved it by measurement. `FdbError::from_code(1020).is_retryable_not_committed()` is
**true**, so `blind_commit_step` (`crates/metadata-fdb/src/lib.rs:864`) routes 1020 into
`BlindStep::Retry` and `outcome_from_commit_error(err, false)` can never see it. The
`conditional = false` argument at that callsite is a **semantically inert** mutation site.

I did not delete the guard — the maintainer said keep it — but I stopped claiming coverage
that does not exist. The module doc now has a section, *"What actually keeps a blind batch
out of `Conflict`"*, which says plainly that the guard is **defence-in-depth and structurally
unreachable**, and names the three mechanisms that are reachable, each with the test that
kills it. `outcome_from_commit_error`'s own doc says the same at the callsite. The
`contention.rs` step-2 assertion that was the vacuous twin
(`classify_commit_error(1020, false) == Fault`) is **deleted**, replaced by a comment
explaining why asserting it would pin nothing; the reachable direction
(`classify_commit_error(1020, true) == Conflict`) stays, because `commit_conditional` really
does call it that way.

**(b) The coverage.** The routing rule `let conditional = !batch.preconditions.is_empty()`
is now the production function `commit_path` (`crates/metadata-fdb/src/lib.rs:1029`), and
`MetadataStore::commit`'s body is exactly `route_commit(self, batch).await` (`:1165`) — so
there is no second, drifting copy of the decision. `route_commit` (`:1053`) dispatches
through the `CommitPaths` seam (`:1044`), whose production impl is `FdbMetadataStore`.

The unit tests at `:1413` and `:1432` drive `route_commit` with a `RecordingPaths` target.
This is the **same seam shape the iteration-2 sign-off explicitly accepted** for
`blind_commit_loop`/`BlindCommit` ("the 1021 no-blind-retry rule is now bound by the
production blind_commit_loop/blind_commit_step"). Only the *destination* is scripted;
`commit_path` and `route_commit` are production. It is not a live test because the routing
rule is **invisible from outside a cluster**: a blind batch sent down the conditional path
still commits (write-only ⇒ empty read-conflict set ⇒ the resolver never rejects it); it
merely loses its bounded retry of 1007/1009, silently. That is the reviewer's own stated
cost, and it is now what RED-A fails on.

`a_blind_commit_that_keeps_losing_is_err_never_conflict` (`:1330`) is the third mechanism:
an exhausted blind retry is `Err`, never `Ok(Conflict)`. This is the assertion that actually
binds the invariant's third clause on the reachable blind path.

### Gap 4 (low, "fix while in there if cheap") — backoff after the final attempt

`blind_commit_loop` (`:902`), `get` and `scan` now `break` before the last `reset`/`on_error`,
so `MAX_ATTEMPTS` attempts cost `MAX_ATTEMPTS - 1` backoffs. Asserted at
`crates/metadata-fdb/src/lib.rs:1309-1313` (`target.resets == MAX_ATTEMPTS - 1`), so the fix
cannot silently regress.

### Gap 5 (low) — inconsistent isolation prefix

`tests/conformance.rs` now stamps `…/{tag}/{nanos}/` like `contention.rs` and `scan.rs`. The
stamp is taken **once per process** so the seven `run_all` clauses still differ only by
`tag`, which is what the shared suite intends. Comment records that `compose down -v` masks
this today, and that a test's isolation should not depend on its harness wiping the disk.

### Gap 6 (low) — `blind_write_race_never_reports_conflict` was near-tautological

The reviewer was right: its `Err(_) => {}` arm swallowed faults and its only surviving
assertion was `committed >= 1`. Now (`tests/contention.rs:86`): a fault **panics** rather
than being swallowed; all `WRITERS` must commit (the grounded FDB property — a write-only
transaction has an empty read-conflict set, so the resolver cannot reject it); and the key
is **read back** and must hold some writer's bytes, which kills a driver whose blind
`commit` returned `Ok(Committed)` without staging the batch. Its doc comment now states
explicitly what it proves (the *premise*) and what it does not (the rule).

I also strengthened `blind_batch_commit_error_surfaces_as_err_never_conflict` step 3
(`tests/contention.rs:121`): the surfaced `Err` must downcast to `FdbError` with code
`2103` — so a blind commit failing for some *other* reason no longer satisfies "surfaces as
Err" — and the key must be absent afterwards (the failed atomic batch applied nothing).

### Gap 7 — xtask's 5× retry

Already fixed in iteration 2 (`run_fdb_leg` runs each leg exactly once, deliberately unlike
`run_tikv_test`). Left as is; its doc comment explains why.

---

## 2. Demonstrated RED → GREEN (all against a live `foundationdb/foundationdb:7.3.77`)

Every red below was produced by mutating the **post-fix tree**, running the project's runner
(`./engine/xtask.sh fdb-conformance`, which brings the cluster up and tears it down), and
restoring. This is the house demonstrated-red pattern
(`crates/metadata-conformance/tests/demonstrated_red.rs`, #419). The brief forbids resting
red on "the crate did not exist yet" (brief.md:39-40) — none of these do.

### RED-A — the routing rule (closes gap 3b; the mutant that survived iteration 2)

Mutation: `commit_path` → always `CommitPath::Conditional` (i.e. `let conditional = true`).

```
thread 'store::tests::a_blind_batch_routes_to_the_blind_path' panicked at crates/metadata-fdb/src/lib.rs:1416:13:
assertion `left == right` failed: a batch with no preconditions is a blind write: it must take
the blind path, which is what gives it a bounded retry and keeps it out of Conflict
  left: Some(Conditional)
 right: Some(Blind)
thread 'store::tests::commit_path_is_decided_solely_by_the_presence_of_preconditions' panicked at crates/metadata-fdb/src/lib.rs:1454:13:
assertion `left == right` failed
  left: Conditional
 right: Blind
```

Iteration 2's report: *"forcing `let conditional = true` at lib.rs:746 is ALSO green on all
four legs."* It is now red on two tests.

### RED-B — the typed exhaustion error (closes gap 2)

Mutation: `blind_commit_loop`'s exhaustion arm reverted to the iteration-2
`BoxError::from(format!("metadata blind commit exhausted {MAX_ATTEMPTS} attempts …"))`.

```
thread 'store::tests::an_exhausted_retry_budget_carries_the_last_fdb_error_as_its_source'
panicked at crates/metadata-fdb/src/lib.rs:1358:18:
exhaustion must be a typed error, not a formatted String
test result: FAILED. 25 passed; 1 failed
```

### RED-C1 — the scan cap is never applied at the production callsite (closes gap 1)

Mutation: `scan_once` passes `usize::MAX` instead of `self.scan_cap` — i.e. `after_page` and
all five of its unit tests remain **green and untouched**, while the store no longer enforces
anything.

```
--lib        test result: ok. 26 passed; 0 failed      <-- pure unit tests do NOT catch it
conformance  test result: ok.  1 passed; 0 failed
contention   test result: ok.  3 passed; 0 failed
thread 'a_scan_past_the_cap_fails_loud_and_returns_no_partial_results' panicked at crates/metadata-fdb/tests/scan.rs:223:28:
an over-cap scan returned Ok(600 keys) — it must fail loud with ScanCapExceeded and return NO
partial result set (#262, ADR-0011)
scan         test result: FAILED. 1 passed; 1 failed
```

This is the direct evidence for the design choice above: a pure `after_page` unit test (the
TiKV peer's shape) would **not** have caught this. The live `tests/scan.rs` leg is the sole
witness. Note also that `--lib`, `conformance` and `contention` all stay green — exactly the
blind spot the reviewer described.

### RED-C2 — silent truncation is caught by the type system

Mutation: `scan_once`'s cap arm returns `Ok(out)` (the accumulated partial `Vec`) instead of
`Err(ScanFailure::CapExceeded)`.

```
error: variant `CapExceeded` is never constructed
error: could not compile `wyrd-metadata-fdb` (lib test) due to 1 previous error
```

Recorded honestly: this mutation is caught at **compile** time (`[workspace.lints.rust]
warnings = "deny"`, root `Cargo.toml:195`), not by an assertion. The `ScanFailure` enum
exists partly for that reason. RED-C4 is the behavioural version.

### RED-C4 — the breach is swallowed as `Ok` (compiles clean)

Mutation: `scan`'s `CapExceeded` arm constructs-and-drops the `ScanCapExceeded` (so the
import stays used) and returns `Ok(Vec::new())`.

```
--lib / conformance / contention: all ok
thread 'a_scan_past_the_cap_fails_loud_and_returns_no_partial_results' panicked at crates/metadata-fdb/tests/scan.rs:223:28:
an over-cap scan returned Ok(0 keys) — it must fail loud with ScanCapExceeded and return NO
partial result set (#262, ADR-0011)
scan         test result: FAILED. 1 passed; 1 failed
```

Together C1 + C4 bind both halves: the cap is *applied*, and the breach yields **no partial
results**.

### RED-D — the headline 1020 → `Conflict` rule (the brief's Falsifiability clause)

Mutation, at the **production callsite** rather than in the classifier, so the `--lib` leg
stays green and the live leg is the sole witness: `commit_conditional`'s
`outcome_from_commit_error(*err, true)` → `(*err, false)`.

```
--lib        test result: ok. 26 passed
conformance  test result: ok.  1 passed          <-- see §3: conformance CANNOT witness this
thread 'conditional_race_loser_yields_conflict' panicked at crates/metadata-fdb/tests/contention.rs:201:27:
writer 1 surfaced a fault instead of a Conflict: Transaction not committed due to conflict
with another transaction
contention   test result: FAILED. 2 passed; 1 failed
```

A real 1020 from a real `fdbserver`, eight racing clients, one winner.

(I first negated `classify_commit_error` itself; that reds `--lib` at
`classify::tests::a_conditional_batch_that_loses_a_race_is_a_conflict` and the runner stops
before the live leg, which is why I moved the mutation to the callsite — a strictly stronger
demonstration.)

### GREEN — restored tree, live cluster, all four legs

```
$ ./engine/xtask.sh fdb-conformance
--lib        test result: ok. 26 passed; 0 failed
conformance  test result: ok.  1 passed; 0 failed   (run_all, all 7 contracts)
contention   test result: ok.  3 passed; 0 failed
scan         test result: ok.  2 passed; 0 failed
xtask fdb-conformance: FoundationDB passed the shared MetadataStore conformance suite and the
contention properties
```

### GREEN — feature OFF (the Check-beat gate)

```
$ ./engine/xtask.sh ci
xtask ci: all checks passed
```

- `ldd target/debug/wyrd | grep -i fdb` → **no `libfdb_c`**. The default build links nothing.
- `grep -c '^name = "foundationdb' Cargo.lock` → `5` (the optional dep tree lands in the lock
  even feature-off), so `cargo deny check` — which `run_ci` invokes,
  `xtask/src/main.rs:1322` (`cargo_deny_check()`, defined at `:1418`) — really does see it.
  `advisories ok, bans ok, licenses ok, sources ok`.

---

## 3. Carried forward, unchanged — the brief defect I am *not* fixing by reshaping the suite

brief.md:124-125 designates `tests/conformance.rs` the "primary — must go red→green" witness
for the 1020 → `Conflict` rule. **It cannot be**, and RED-D above is the third independent
measurement of that: under the callsite negation the conformance leg stays **green**.
`contract_rename_race_yields_conflict` (`crates/metadata-conformance/src/lib.rs:168-227`) is
strictly sequential, so the racer's precondition read observes the deleted key and returns
`Ok(Conflict)` from the observed-miss branch (`crates/metadata-fdb/src/lib.rs:783`);
`classify_commit_error` is never invoked from `conformance.rs` and FDB error 1020 never
arises there.

`tests/contention.rs:63` is the live witness. Per the brief's own invariant
(brief.md:51-54) I did **not** fork or weaken `crates/metadata-conformance` to satisfy the
brief's sentence — the shared suite is untouched by this patch (`git diff --stat` shows no
`crates/metadata-conformance` change), and `redb` and `tikv` stay green under `xtask ci`.

Also carried forward for the Check beat: the C4-verify gate row has twice been scored "pass"
on the evidence string *"no pre-patch state to isolate a RED against"* — verbatim what
brief.md:39-40 forbids. Real reds exist and are cited above (RED-A, RED-B, RED-C1, RED-C4,
RED-D, each with file:line and panic text). The row should cite them. Note too that
`run_ci` builds with **default** features and the entire `store` module is
`#[cfg(feature = "fdb")]` (`crates/metadata-fdb/src/lib.rs:546`), so a green `C4-ci` never
compiled the driver, never linted it, and ran none of the four legs — including the
`store::tests` that are the sole witness for the routing rule, the 1021 rule and the
exhaustion rule. `C4-ci` is not coverage for this crate's driver.

---

## 4. Alternatives rejected, with costs

**Fork/weaken the shared suite so `conformance.rs` witnesses 1020.** Forbidden by the brief's
Invariant-to-restore (brief.md:51-54) and would be self-defeating: the suite is the thing
that makes the invariant quantified over *all* backends. Not done.

**Make `outcome_from_commit_error(err, false)` reachable with 1020**, so the `conditional`
guard becomes load-bearing. This would mean excluding 1020 from `blind_commit_step`'s retry
arm — i.e. *removing the bounded retry of a definitively-not-committed blind batch* in order
to create a test target. Cost: a blind commit that loses one resolver race would fail
outright instead of retrying, on every caller. Rejected: manufacturing coverage by deleting
correct behaviour. Instead I documented the guard as unreachable defence-in-depth and bound
the clause where it *is* reachable (three mechanisms, three tests).

**Delete the guard entirely**, since no test can pin it. Rejected: the maintainer's
iteration-2 ruling was "The guard itself is harmless — keep it; it is the DOC CLAIM and the
coverage that are refuted." Doing otherwise would be re-litigating a settled call. Cost of
keeping it: one `bool` argument, ~0 runtime; it catches a future refactor that stops routing
1020 through the retry gate.

**Unit-test `after_page` only (the TiKV peer's exact test shape).** Concretely refuted by
RED-C1: with `after_page`'s five unit tests green and untouched, replacing `self.scan_cap`
with `usize::MAX` at the one production callsite leaves `--lib`, `conformance` and
`contention` all green. The wiring needs the live test.

**A `#[cfg(test)]`-gated cap override instead of `with_scan_cap`.** Cost: it would not exist
in the `--test scan` binary's dependency (integration tests link the crate as an external
dependency, without `cfg(test)`), so the live test could not reach it at all. `with_scan_cap`
is the same public-builder shape as `with_prefix`, which the sign-off already accepted, and
it clamps upward at `SCAN_CAP` so it cannot loosen the constraint.

**Adopt `Database::run`'s closure-retry.** Named as a hazard by the brief (brief.md:83-86):
it re-runs on 1021. Not used; `blind_commit_loop` is hand-rolled precisely so 1021 cannot
enter the retry arm.

---

## 5. Forced self-refutation (the three questions)

**(a) Genuine red? — YES.** Every new binding was reverted individually against a live
cluster and went red, with the panic text captured verbatim in §2: RED-A (routing rule, 2
tests), RED-B (typed exhaustion), RED-C1 (cap not applied), RED-C4 (breach swallowed as
`Ok`), RED-D (1020 classification, the brief's Falsifiability clause). RED-C2 is caught at
compile time and is recorded as such rather than dressed up as an assertion failure. The
tree was restored from a pristine copy after each mutation and the final green run in §2 is
on the restored tree.

**(b) Production path? — YES.**
- `tests/conformance.rs`, `tests/contention.rs`, `tests/scan.rs` construct the real
  `FdbMetadataStore` over a real `Database` against a real `fdbserver` and call the real
  `MetadataStore::get/scan/commit`. `conformance.rs` drives the **shared, unforked** `run_all`.
- `tests/scan.rs:74` drives the production `scan` → production `scan_once` → production
  `paging::after_page` → production `ScanCapExceeded`. Only the cap *number* is lowered.
- `store::tests` drive the production `route_commit`, `commit_path`, `blind_commit_loop`,
  `blind_commit_step` and `outcome_from_commit_error`. The `FdbError`s are built with
  `FdbError::from_code`, so `is_retryable_not_committed()` is answered by `libfdb_c` itself,
  not by a hand-written table. Only the commit *destination* / error *source* is scripted —
  the same seam shape the iteration-2 sign-off explicitly accepted.
- No mock, copy, or re-implementation of the driver exists anywhere in the patch.

**(c) Fixture includes the fault? — YES.**
- `contention.rs` races 8 real clients on a multi-thread runtime for one key and consumes a
  real FDB **1020** — *grounded first* by driving the raw client through a deterministic lost
  race and asserting the server's code really is `NOT_COMMITTED` before anything is
  classified on it (`contention.rs:121`, step 1).
- The blind-fault leg provokes a real server-side **2103 `value_too_large`** and now asserts
  the surfaced error downcasts to `FdbError` with that exact code — so an unrelated failure
  can no longer satisfy "surfaces as `Err`".
- `scan.rs` **grounds its own page-boundary fixture** against the live server (asserts FDB
  actually reports `more()` on the first page, so a knob change cannot quietly disarm the
  paging test) and **grounds the cap fixture** (asserts the *uncapped* store sees all 600
  keys, so the capped store's `Err` cannot pass for the wrong reason — missing data).
- Nothing is curated out: `blind_write_race` no longer swallows `Err`, and now requires all 8
  writers to commit and reads the key back.

One honest limit, stated rather than hidden: `blind_write_race_never_reports_conflict`'s
`Ok(Conflict)` panic arm is **unreachable on a healthy cluster**, because FoundationDB never
rejects a write-only transaction. Its doc comment now says so. It contributes the *premise*
(grounded live), not the rule; the rule is bound by `store::tests`. This is disclosed, not
relied upon.

---

## 6. Commit-readiness

Run against the post-patch tree in `$PDCA_WORKTREE`:

- `cargo fmt --all -- --check` → clean.
- `cargo clippy -p wyrd-metadata-fdb --features fdb --all-targets -- -D warnings` → clean.
- `cargo clippy -p wyrd-metadata-fdb --all-targets -- -D warnings` (feature off) → clean.
- `cargo doc -p wyrd-metadata-fdb --no-deps` **and** `--features fdb` → clean, both ways.
  This caught a real defect I introduced: the workspace's `[workspace.lints.rust] warnings =
  "deny"` (root `Cargo.toml:195`) reaches rustdoc, so `[`MAX_ATTEMPTS`]` / `[`commit_path`]`
  / `[`route_commit`]` links from **public** doc comments to **private** items were hard
  errors, and `[`RetryBudgetExhausted`]` in the crate doc was an unresolved link with the
  feature **off**. `cargo doc` is not an `xtask ci` gate, so no PDCA gate models it. Fixed.
- `./engine/xtask.sh ci` → `xtask ci: all checks passed`.
- `./engine/xtask.sh fdb-conformance` → all four legs green (§2).
- `patch.diff` verified to `git apply --check` cleanly onto `182ae4f` in a scratch checkout.

`deny.toml` is unchanged from iteration 2 (the ISC entry the maintainer ruled in, with its
ADR-0003 §2 rationale). Per the iteration-2 ruling I did **not** touch its comment.

## 7. STOP discipline

No branch pushed, no PR opened, nothing marked ready. `patch.diff` + the three test files +
these notes are the whole handoff.
