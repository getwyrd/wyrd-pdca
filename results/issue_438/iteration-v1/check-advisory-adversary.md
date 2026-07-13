# Adversarial review — issue 438 / metadata-fdb

Advisory only; nothing here gates. `libfdb_c` 7.3, `fdbcli` and Docker were all present on
this host, so the asserted red→green was **re-run for real** against
`foundationdb/foundationdb:7.3.77`, not reasoned about. Four mutations were applied to a
scratch copy of the target and driven against a live single-node `fdbserver`.

Baseline reproduced: `cargo build -p wyrd-metadata-fdb --features fdb` compiles; both test
binaries pass against the live cluster; `fdbcli getrangekeys` confirms real keys under
`wyrd-fdb-conformance/<pid>/<tag>/` — the suite drives the production driver, not a double.
`blind:oversized` is absent, confirming the blind fault really faulted server-side.

## Refutations

- **NEEDS-HUMAN** — `crates/metadata-fdb/tests/conformance.rs:27`: the brief's prescribed RED
  for the **primary** test file does not reproduce. `brief.md:26-28` asserts "deliberately
  negate the 1020→`Conflict` classification (return `Err` instead) and
  `contract_rename_race_yields_conflict` **fails**". I applied exactly that mutation (deleted
  the `Conflict` arm at `crates/metadata-fdb/src/lib.rs:116-118`) and ran the primary file
  against a live `fdbserver`: **`trait_contract_against_fdb ... ok`, 1 passed, 0 failed.**
  Cause: `contract_rename_race_yields_conflict`
  (`crates/metadata-conformance/src/lib.rs:168-227`) is strictly sequential — the winner's
  `commit` is awaited before the racer's — so the racer's precondition read at
  `crates/metadata-fdb/src/lib.rs:498` observes the deleted key and returns `Ok(Conflict)` from
  the **observed-miss** branch at `crates/metadata-fdb/src/lib.rs:505`. `classify_commit_error`
  is never invoked from `conformance.rs`; FDB error 1020 never arises there. The clause the
  brief names as the red→green witness is structurally incapable of exhibiting it. The 1020
  rule is pinned **only** by the secondary file (`crates/metadata-fdb/tests/contention.rs:175`,
  which did fail under the mutation, as did `contention.rs:288-292`). The fix is not wrong —
  the *evidence assignment* is: `brief.md:124-125` designates `conformance.rs` "primary — must
  go red→green"; it cannot.

- **NEEDS-HUMAN** — `check-gates.json:42-48`: the `C4-verify` row is recorded `"result":
  "pass"` with the rationale "no pre-patch state to isolate a RED against; C4-ci gates the
  whole tree (#88)". That is verbatim the rationalization `brief.md:39-40` pre-emptively
  forbids: "the RED is a *real failing assertion against a real server*, not criterion-absence
  — Do MUST capture it, not rest red on 'the crate did not exist yet'." A non-gating row
  scored `pass` on the one ground the plan ruled out is an unwarranted claim, and `C2
  Reproduction` is `"none" / "(no gate configured)"`, so no red was mechanically captured
  anywhere. The green is real (I reproduced it); the **red** is asserted, not evidenced.

- **NEEDS-HUMAN** — `crates/metadata-fdb/src/lib.rs:531`: the brief's headline constraint —
  "`1021 commit_unknown_result` MUST NOT be blind-retried" (`brief.md:83-86`) — is enforced by
  **no test in this patch**. Concrete failing case: change the blind path's guard from
  `err.is_retryable_not_committed()` to `err.is_retryable()` (1021 *is* retryable — that is
  precisely why `Database::run` re-runs on it, as `lib.rs:38-42` says). I applied that
  mutation. Result: `conformance` 1 passed / 0 failed; `contention` **4 passed / 0 failed**;
  `--lib` 12 passed / 0 failed. The named guardian test,
  `commit_unknown_result_is_never_conflict_and_never_retried`
  (`crates/metadata-fdb/tests/contention.rs:104-107`), never touches `commit_blind`: it calls
  `assert_unknown_result_is_distinguishable` (`contention.rs:321-348`), which only asserts on
  the pure `classify_commit_error` and a `Display` substring — duplicating the unit tests at
  `crates/metadata-fdb/src/lib.rs:174-208`. A test named for a property it does not exercise.
  (Feature-off it degrades further, to `feature_off()`'s `eprintln!` at
  `contention.rs:406-417`.) The invariant holds today by inspection of one token; nothing
  defends it.

- **NEEDS-HUMAN** — `crates/metadata-fdb/src/lib.rs:480`: `scan`'s paging loop is dead code
  under test. Concrete failing case: replace the `next_range` match with `return Ok(out)` — a
  `scan` that silently returns only its **first page**. I applied it: `conformance` 1 passed /
  0 failed, `contention` 4 passed / 0 failed. Every scan in the shared suite returns ≤3 keys
  in one page, so `more()` is never true and `scan_once`'s loop
  (`crates/metadata-fdb/src/lib.rs:473-485`) never iterates. The TiKV peer this slice was told
  to mirror carries a dedicated at-scale proof for exactly this,
  `crates/metadata-tikv/tests/scan.rs` (the #254 "paged-scan completeness" binary named at
  `xtask/src/main.rs:218-221`); `crates/metadata-fdb/tests/` has `conformance.rs` and
  `contention.rs` only. A silent truncation in the driver's most consequential read path ships
  green.

- `crates/metadata-fdb/tests/contention.rs:64` — `blind_write_race_never_reports_conflict` is
  vacuous with respect to the guard it exists to pin. Dropping `conditional &&` at
  `crates/metadata-fdb/src/lib.rs:116` (so a blind 1020 becomes `Conflict`) leaves it **ok**.
  It cannot bite: as the crate's own doc concedes (`crates/metadata-fdb/src/lib.rs:33-35`), a
  write-only FDB transaction has an empty read-conflict set and "cannot be rejected with 1020
  at all", so the guard is **production-unreachable** on FDB. The mutation is caught only by
  `classify::tests::a_blind_batch_is_never_a_conflict`
  (`crates/metadata-fdb/src/lib.rs:163-171`, a feature-off pure-function unit test) and the
  direct classifier assert at `contention.rs:293` — neither of which is "a real failing
  assertion against a real server". Compounding: `contention.rs:232` accepts `Err(_) => {}`
  for *any* writer, so seven of eight blind writers failing outright still passes. Production
  reach for the blind clause rests entirely on step 3 (`contention.rs:304-316`, the real 2103
  `value_too_large`), which *does* bind — that one I could not refute.

- **NEEDS-HUMAN** — `crates/metadata-fdb/src/lib.rs:608-626`: `scan` holds one transaction
  across every page (deliberately, for the consistent cut, `lib.rs:599-607`) and on a
  retryable error restarts the whole scan from scratch (`lib.rs:619`). FDB's 5 s transaction
  limit therefore makes any scan whose range read exceeds 5 s **permanently** unsatisfiable:
  `1007 transaction_too_old` → `is_retryable()` → restart → time out again → after
  `MAX_ATTEMPTS` a bare `Err`. Concrete case: a directory whose dirent listing takes >5 s to
  range-read can never be listed, and the retry loop costs 5× before saying so. `brief.md:98-102`
  defers "the transaction-envelope check" to #437 on the ground that "single `WriteBatch`
  commits sit far inside those limits" — that reasoning covers `commit`, not `scan`, whose
  envelope is unbounded in the size of the prefix. Worth a human call on whether the deferral
  as written actually covers the read path.

- `xtask/src/main.rs:417` — `run_fdb_test` retries a failing test binary five times, treating
  an **assertion failure** identically to a cluster-still-settling connection failure. Given
  the first finding, `conditional_race_loser_yields_conflict` is now the *sole* test pinning
  the 1020→`Conflict` rule, and this loop converts any future flake in it into a green. Not a
  live defect — I ran that test 8× against the live cluster and it passed 8/8 (the property is
  genuinely deterministic: a late racer observed-misses, an early one loses the resolver race;
  either way `Conflict`). Flagged because it mirrors the TiKV precedent at
  `xtask/src/main.rs:242` that the brief instructed Do to copy, so it is inherited, not
  invented — but it now guards a strictly narrower test set than TiKV's three binaries do.

## Attempted and could not refute

- **The driver is built and exercised, not scaffolded.** `cargo build -p wyrd-metadata-fdb
  --features fdb` links `libfdb_c` and compiles; both binaries run green against a real
  7.3.77 `fdbserver`; `fdbcli getrangekeys` shows the seven per-clause prefixes and the three
  contention prefixes actually written. The `brief.md:144-146` forcing function is met.
- **Exactly-one-winner under the 8-way conditional race** (`contention.rs:178-183`): stable
  across 8 consecutive runs. I could not construct an interleaving that yields two winners or
  an `Err`; a racer that takes its read version after the winner commits observed-misses to
  `Conflict` at `lib.rs:505`, one that takes it before is rejected 1020 → `Conflict`.
- **Blind-batch-`Err` on the production path**: mutating `outcome_from_commit_error`
  (`lib.rs:567-575`) to launder a blind fault into `Conflict` is caught by `contention.rs:310-316`
  via a genuine server-side 2103. This clause's production reach is real.
- **Put/delete ordering** matches both peers — `crates/metadata-fdb/src/lib.rs:551-558` applies
  puts then deletes, as do `crates/metadata-redb/src/lib.rs:89-92` and
  `crates/metadata-tikv/src/lib.rs:582-587`. No cross-backend divergence introduced.
- **Dependency bookkeeping** is as the brief specified: `Cargo.toml:108` pins `foundationdb =
  { version = "0.10", default-features = false, features = ["fdb-7_3"] }`; `Cargo.lock:1317-1319`
  carries `0.10.0`, so `cargo deny` in `run_ci` does see the tree feature-off, as claimed.
  `deny.toml` is untouched.
- **Feature-off cleanliness**: the `#[cfg(not(feature = "fdb"))]` arms
  (`conformance.rs:67-74`, `contention.rs:388-417`) compile and skip; the shared suite was
  **not** forked or weakened — `crates/metadata-conformance/src/lib.rs` is unmodified by this
  diff, so redb and tikv are unaffected.
- The `OnceLock<NetworkAutoStop>` one-network-per-process handling (`lib.rs:388-406`) survived
  seven `make_store` calls in one process and three multi-thread runtimes in a parallel
  `cargo test`; I could not provoke a double-boot or a use-after-stop.

## Bottom line

The **fix** withstood attack: the driver's conditional-race and blind-fault behaviour against
a real cluster is correct, and I could not find an input that breaks the commit contract. The
**evidence** did not: the brief's primary red→green witness is provably inert (finding 1), the
`C4-verify` gate rationalizes its absence on the one ground the plan forbade (finding 2), and
two of the brief's three named constraints — no blind-retry of 1021, scan completeness — pass
their own mutants (findings 3, 4). Three of the four mutations I applied shipped green.
