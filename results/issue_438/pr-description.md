# PR description

## Summary
**User impact:** Wyrd cannot keep its metadata in FoundationDB today. FoundationDB was
chosen as the production distributed metadata store, but nothing implements that choice —
so an operator who wants to run Wyrd against a real distributed cluster is left with the
embedded single-node option, and none of the work that depends on the FoundationDB store
(the CI harness, choosing the backend when the server starts, client packaging, the #257
go/no-go battery) can begin.

This PR adds the FoundationDB metadata backend as a new crate, behind a Cargo feature that
is **off by default**, so existing builds and deployments are unchanged.

Reported in #438.

## What to look at
The crux is how the new backend answers one question: *did a write fail because someone
else got there first, or because the database broke?* A batch that carries preconditions
(a compare-and-swap) and loses a race must report a conflict, so the caller can re-read and
retry. A batch with no preconditions has no precondition to have failed, so it must report
an error the caller cannot ignore — never a conflict, which a caller checking only for
success would read as "it worked". Reviewing the commit path and the tests around it is the
highest-value first pass.

Two more properties are worth a look: a commit whose outcome the client cannot determine is
never retried (the batch may already have landed, and it is not guaranteed idempotent), and
a listing that runs past its size ceiling fails loudly instead of returning a partial one.

To try it, with the FoundationDB 7.3 C client on the build host and Docker running:

```
cargo xtask fdb-conformance   # brings up a single-node cluster, runs all legs, tears down
```

To confirm nothing changed for anyone not opting in:

```
cargo xtask ci                # green; the default build links no libfdb_c
```

## Root cause
`crates/metadata-fdb` does not exist on `main` (`git ls-tree origin/main crates/` has no
`metadata-fdb`), so the `MetadataStore` trait — implemented by `redb` and `tikv` — has no
FoundationDB implementation and no FoundationDB conformance run. The decision recorded in
ADR-0042 is therefore unexecuted, and every downstream milestone item is blocked on the
missing store rather than on anything undecided.

## Fix
Adds `crates/metadata-fdb` with an `FdbMetadataStore` implementing the **unchanged**
`MetadataStore` trait over the `foundationdb` crate — composition, not a refactor
(ADR-0010). No trait change; the shared conformance suite is untouched, so `redb` and
`tikv` keep passing the same seven contracts.

- The `fdb` feature is off by default and all backend dependencies are optional, mirroring
  `crates/metadata-tikv/Cargo.toml`. Both new test binaries skip cleanly when no cluster
  file is configured, so `cargo xtask ci` stays green on a machine with no FoundationDB.
- FDB error `1020 not_committed` maps to `Ok(CommitOutcome::Conflict)` only for a batch
  that carried preconditions; a blind batch surfaces `Err`. Error `1021
  commit_unknown_result` is surfaced as its own typed error, never retried and never a
  conflict. The `foundationdb` crate's `Database::run` closure-retry is deliberately not
  used, because it re-runs on 1021.
- `scan` holds one read version across FDB's internal pages and is bounded by the sibling
  backend's `SCAN_CAP` (2^20). A breach returns `Err(ScanCapExceeded)` and **no** partial
  `Vec`: a silently truncated `inode:` scan corrupts GC's never-reclaim safety set (#262,
  ADR-0011).
- Every `Err` is downcastable to something more specific than a string, including the
  retry-exhaustion error, whose `source()` is the last `FdbError` — so a caller can tell a
  transient `transaction_too_old` from a permanent `value_too_large`.
- `deploy/fdb-single-node/docker-compose.yml` (outside the Cargo workspace, ADR-0010) and
  `cargo xtask fdb-conformance` bring the cluster up, run the legs feature-on, and always
  tear it down.

**New dependency (maintainer audit, ADR-0003 / INTEGRATION §4):** `foundationdb = "0.10"`,
`default-features = false`, `features = ["fdb-7_3"]`, matching the FoundationDB 7.3 client
and server. Its build-time graph reaches `libloading` under **ISC** (via `foundationdb-sys`
→ `bindgen` → `clang-sys`), added to `deny.toml`'s allowlist with an ADR-0003 §2 rationale.
No ISC-licensed code is linked into a shipped Wyrd binary.

## Verification
Line references are to `main` at `182ae4f97ab8a3e10f0438597c67559a3e11a393` for existing
code, and to the files this PR adds where noted.

- **Claim:** FoundationDB satisfies the same `MetadataStore` contract `redb` and `tikv` do,
  via the shared, unforked suite.
  - **Checked:** `crates/metadata-conformance/src/lib.rs:291` on `main` — `run_all`, the
    single shared runner, is unchanged by this PR (`git diff --stat` shows no
    `crates/metadata-conformance` change), so `redb` and `tikv` are still held to it.
  - **Test:** `crates/metadata-fdb/tests/conformance.rs:27` (added) drives all seven
    contracts against a live `foundationdb/foundationdb:7.3.77` — passes. `cargo xtask ci`
    is green with the feature off.

- **Claim:** A conditional batch that loses a race yields `Ok(Conflict)`; a blind batch
  never does.
  - **Checked:** `crates/traits/src/lib.rs:346-350` on `main` states the rule; the sibling
    backend applies the identical routing at `crates/metadata-tikv/src/lib.rs:542-546` on
    `main`. Reproduced at `crates/metadata-fdb/src/lib.rs:1029` (`commit_path`) and
    `:1053` (`route_commit`), which is the whole body of `commit` — so there is no second,
    drifting copy of the decision.
  - **Test:** `crates/metadata-fdb/tests/contention.rs:63` — eight racing clients against
    the live server, exactly one winner, the losers carrying a real FDB 1020. Negating the
    classification at the production callsite makes it fail (`contention.rs:201`: *"writer
    N surfaced a fault instead of a Conflict"*); it passes as written. Forcing every batch
    down the conditional path makes `src/lib.rs:1456` fail.

- **Claim:** `1021 commit_unknown_result` is never retried and never a conflict.
  - **Checked:** the retry gate at `crates/metadata-fdb/src/lib.rs:864` re-applies only
    `retryable_not_committed` errors, so 1021 — the one that may already have landed —
    cannot re-enter the loop at `:902`.
  - **Test:** the unit tests drive the *production* retry loop with a real `FdbError`.
    Widening the gate from `is_retryable_not_committed()` to `is_retryable()` (1021 *is*
    retryable) makes them fail.

- **Claim:** A scan past the cap fails loud and returns no partial results.
  - **Checked:** `crates/metadata-tikv/src/lib.rs:136-145` on `main` states this as *"a
    correctness constraint, not a tuning knob"*. The same `1 << 20` constant is at
    `crates/metadata-fdb/src/lib.rs:441`, enforced after each page and before FDB's cursor
    is consulted at `:755`, and returned as `Err(ScanCapExceeded)` with no partial `Vec` at
    `:1130`.
  - **Test:** `crates/metadata-fdb/tests/scan.rs:74` grounds its fixture against the live
    server (an uncapped store really does see all 600 keys) and then fails if the cap is not
    applied at the production callsite, or if the breach is swallowed as `Ok`
    (`scan.rs:223`). Unit-testing the decision function alone does **not** catch either.

- **Claim:** Default builds are unaffected — no `libfdb_c`, no behaviour change.
  - **Checked:** `default = []` with every backend dependency `optional = true` in
    `crates/metadata-fdb/Cargo.toml`, the shape `crates/metadata-tikv/Cargo.toml` uses on
    `main`. No `open_fdb_meta` selection arm is added to `crates/server/src/cli.rs`.
  - **Test:** `cargo xtask ci` → all checks passed; `ldd target/debug/wyrd | grep -i fdb` →
    empty. `cargo deny check` still sees the new dependency tree, because an optional dep
    lands in `Cargo.lock` even feature-off → `advisories ok, bans ok, licenses ok, sources
    ok`.

**Two limits, stated rather than hidden.** The shared suite's rename-race clause is
sequential, so it cannot itself witness the 1020 classification — the precondition read
observes the deleted key and returns a conflict without any server-side race. The live
witness is `tests/contention.rs`; the shared suite was deliberately not reshaped to
manufacture one. And `blind_write_race_never_reports_conflict`'s conflict arm is unreachable
on a healthy cluster, because FoundationDB never rejects a write-only transaction; the test
contributes that premise, grounded live, and the rule itself is bound by the commit-path
tests. Both are noted in the code.

Fixes #438
