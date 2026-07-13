# Adversarial review — issue 438 / metadata-fdb (iteration 2)

**Advisory. Gates nothing.** I re-ran the red→green proof for real: `libfdb_c` 7.3, `fdbcli`,
Docker and `cargo` are all present on this host, so I stood up
`foundationdb/foundationdb:7.3.77` from the patch's own
`deploy/fdb-single-node/docker-compose.yml`, ran all four legs against it, and then ran eight
mutations of `crates/metadata-fdb/src/lib.rs` to see which the suite actually catches. The
target worktree was never modified (10 files still dirty, as delivered); mutation work was done
in a scratch copy.

## Mutation matrix (live cluster, all four legs)

| # | mutation | site | `--lib` | conformance | contention | scan |
|---|---|---|---|---|---|---|
| M1 | `is_retryable_not_committed()` → `is_retryable()` | `lib.rs:579` | **RED** | green | green | green |
| M2 | `let conditional = !batch.preconditions.is_empty()` → `true` | `lib.rs:746` | green | green | green | green |
| M3 | `outcome_from_commit_error(err, false)` → `(err, true)` | `lib.rs:613` | green | green | green | green |
| M4 | paging loop → `return Ok(out)` (first page only) | `lib.rs:487` | green | green | green | **RED** |
| M5 | precondition read `snapshot=false` → `true` | `lib.rs:505` | green | green | **RED** | green |
| M6 | drop the `conditional &&` guard | `lib.rs:123` | **RED** | green | **RED** | green |
| M7 | blind `Fault` → `Ok(Conflict)` | `lib.rs:680` | **RED** | green | **RED** | green |
| M8 | observed-miss never returns `Conflict` | `lib.rs:510` | green | **RED** | green | green |

M4 and M6 needed a warning-clean rewrite first — the naive edits fail `deny(warnings)` and go
red *everywhere*, which is a compile error masquerading as a kill. Anyone re-deriving this
matrix should watch for that.

**The two carry-forward gaps are genuinely closed.** M1 (the 1021 no-blind-retry rule,
`brief.md:83-86`) is killed by `lib.rs:817`, driving the *production* `blind_commit_loop` and
`blind_commit_step` — only the error's *source* is scripted, and `FdbError::from_code`'s
retryability is answered by `libfdb_c` itself (I probed it: 1020 → `retryable_not_committed`,
1021 → `maybe_committed`). M4 (paged-scan truncation) is killed by `tests/scan.rs`, which
*grounds* its own fixture at `scan.rs:163` by asserting the live server really reports `more()`.
Contention passed 12/12 reruns — no flake in the exactly-one-winner assertion.

I attempted to break the 1020→`Conflict` rule (M5, M6, M7), the observed-miss path (M8) and the
paging loop (M4) and could not. The findings below are what survived.

## Findings

- **NEEDS-HUMAN — `deny.toml:42-43` states a licence fact that is false.** The rationale
  comment reads "No ISC-licensed code is linked into a shipped Wyrd binary." `cargo tree -i ring
  -e normal --all-features` shows `ring v0.17.14` (`license = "Apache-2.0 AND ISC"`) as a
  **normal** dependency of `wyrd-server` via `tikv-client → tonic → tokio-rustls → rustls`, and
  `untrusted v0.9.0` (`license = "ISC"`, pure) beneath it. So `wyrd-server --features tikv` —
  the production TiKV backend build — links ISC code today, and `NOTICE` carries no ISC
  attribution (`grep -ci isc NOTICE` → 0). The comment even names `ring` in the next sentence
  while denying the conclusion. The *ruling* is fine and the entry is load-bearing (I removed
  only the `"ISC",` line and `cargo deny --all-features check licenses` rejects `libloading`,
  `ring` **and** `untrusted`); it is the ADR-0003 §2 rationale a maintainer will rely on at the
  next audit that is wrong. Either narrow the sentence to the build-time `libloading` path or
  drop it and handle the `untrusted`/`ring` notice obligation.

- **NEEDS-HUMAN — `lib.rs:613` and `lib.rs:746`: two surviving mutants, and the module doc
  overclaims what pins the third clause.** `lib.rs:31` says the `conditional` guard "is the
  whole of the invariant's third clause" and `lib.rs:36` says it "is pinned by
  `crates/metadata-fdb/tests/contention.rs`". Neither holds on the production blind path. I
  probed `libfdb_c`: `FdbError::from_code(1020).is_retryable_not_committed()` is **true**, so
  `blind_commit_step` (`lib.rs:579`) always routes 1020 into `BlindStep::Retry` — meaning
  `outcome_from_commit_error(err, false)` at `lib.rs:613` can *never* be reached with 1020, and
  `classify_commit_error(1020, false)` is unreachable from production. Proof: flipping that
  literal `false` to `true` (M3) leaves all four legs green, and forcing
  `let conditional = true` at `lib.rs:746` (M2) — deleting the TiKV dispatch rule this crate
  says it reproduces — is also green on all four legs. What actually keeps a blind batch out of
  `Conflict` is the retry gate, not the guard. `contention.rs:286-297` "pins" the guard only by
  calling the pure `classify_commit_error` directly — the same shape the previous sign-off
  rejected for the 1021 rule (`brief.md:210` item 2). The guard is harmless defence-in-depth;
  the *doc claim* and the *reviewer's confidence in it* are what I refute. Concrete cost of M2
  surviving: it silently removes the bounded retry of `1007 transaction_too_old` /
  `1009 future_version` from every blind commit, and no test notices.

- **NEEDS-HUMAN — `lib.rs:617-619`, `lib.rs:701-703`, `lib.rs:730-732`: the retry-exhaustion
  paths break the downcast contract this same diff promises at `lib.rs:892-902`.** That
  re-export doc states "a caller that wants to distinguish, say, a transient
  `transaction_too_old` from a permanent `value_too_large` must be able to
  `downcast_ref::<foundationdb::FdbError>()`". Concrete failing case: a blind commit that fails
  five times with `1007 transaction_too_old` (probed: `retryable_not_committed = true`, so every
  attempt takes the `Retry` arm) exits `blind_commit_loop` at `lib.rs:617` as
  `BoxError::from(format!("metadata blind commit exhausted 5 attempts…"))` — a `String`-backed
  error. `downcast_ref::<FdbError>()` returns `None`, and the transient/permanent distinction the
  doc promises is exactly the one destroyed. Same for `get` under five `1007`s and for `scan`
  under five. Contrast `CommitUnknownResult` (`lib.rs:137`), which *is* downcastable — so the
  crate is inconsistent with itself, not merely with a peer.

- **NEEDS-HUMAN — `lib.rs:478-492` (`scan_once`) has no `SCAN_CAP`, unlike its sibling
  distributed backend.** `crates/metadata-tikv/src/lib.rs:136-145` documents the total-results
  cap as "a **correctness constraint, not a tuning knob** (#262)" because "a silently truncated
  `inode:` scan corrupts GC's never-reclaim safety set (data loss)", and fails loud with
  `ScanCapExceeded` past `2^20`. `FdbMetadataStore::scan` accumulates `out` without any ceiling.
  Concrete divergence: on a prefix holding >2^20 entries, TiKV returns `Err(ScanCapExceeded)`
  with no partial `Vec`; FDB either grows the gateway heap without bound or — once the range
  read passes FoundationDB's 5 s transaction limit — takes `1007` at `lib.rs:726`, restarts the
  *whole* scan five times, and finally returns the opaque `"metadata scan exhausted 5 attempts"`
  string. This is **not** the transaction-envelope item `brief.md:98-102` deferred to #437 (that
  carve-out reasons only about `commit`: "Single `WriteBatch` commits sit far inside those
  limits"); the #262 heap/GC bound is a separate constraint, undeclared anywhere in the brief,
  and this diff is the first thing to introduce an FDB `scan`. The shared `run_all` suite does
  not test the cap, so no gate can see this.

- `contention.rs:73` (`blind_write_race_never_reports_conflict`) is close to a tautology and
  cannot witness the rule it is named for. Its own doc concedes the mechanism at
  `contention.rs:69-71`: "A write-only FDB transaction has an empty read-conflict set, so the
  resolver cannot reject it — all N legitimately commit." No error is produced, so the
  `Ok(CommitOutcome::Conflict)` panic arm at `contention.rs:225` is unreachable, and any error
  that *did* occur is swallowed at `contention.rs:230` (`Err(_) => {}`). The test never reads the
  key back, so its only surviving assertion is `committed >= 1`. It would stay green against a
  `commit` that laundered blind 1020s into `Conflict` — the very mutation it is named after; the
  kills under M6/M7 came from its sibling
  `blind_batch_commit_error_surfaces_as_err_never_conflict`. Keep it, but it is documentation,
  not a witness.

- `check-gates.json:42-48` — the C4-verify row is scored `"pass"` on the evidence string *"no
  pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88)"*. This is
  unwarranted three times over. (a) `brief.md:39-40` explicitly forbids resting red on "the
  crate did not exist yet". (b) `brief.md:210` already flagged this exact row at the previous
  sign-off — it recurs verbatim. (c) The claim that C4-ci covers it is false in a way that
  matters: `run_ci` builds with **default** features, and the entire `store` module is
  `#[cfg(feature = "fdb")]` (`lib.rs:361`), so the green C4-ci gate never compiled the driver,
  never linted it, and ran none of the four legs — including the four `store::tests` that are the
  *sole* witness for the 1021 rule. Real reds exist and are cheap to cite: the matrix above.

- `lib.rs:608-616`, `lib.rs:697`, `lib.rs:726` — the loops call `on_error` (FDB's exponential
  backoff, i.e. an actual sleep) after the **final** failed attempt, then immediately fall out to
  the exhaustion error. An exhausted blind commit therefore performs `MAX_ATTEMPTS` sleeps for
  `MAX_ATTEMPTS - 1` usable retries, charging the caller one full backoff it can never spend.
  `a_blind_commit_gives_up_after_max_attempts` (`lib.rs:874`) asserts `attempts` but not
  `resets`, so it does not see this. Low severity; cosmetic to fix (`break` before the last
  `reset`).

- `conformance.rs:58` builds its isolation prefix as `wyrd-fdb-conformance/{pid}/{tag}/` — no
  timestamp — while the two sibling binaries added by this same diff *do* stamp
  (`contention.rs:334`, `scan.rs:197`, both `…/{tag}/{nanos}/`). Concrete case: two conformance
  runs against a cluster that is not torn down between them, from processes that reuse a pid
  (routine inside a container, where pids restart low), collide — `contract_require_absent_gates`
  then meets a key the previous run left behind. `xtask fdb-conformance` masks this today by
  running `compose down -v`. It mirrors the TiKV peer, so it is inherited rather than invented;
  but the diff is internally inconsistent about it.

## What I could not refute

- The 1021 no-blind-retry rule (`brief.md:83-86`): M1 kills `--lib`. The seam
  (`BlindCommit` / `ScriptedCommit`, `lib.rs:597,776`) is **not** a parallel re-implementation —
  `blind_commit_loop` and `blind_commit_step` are the production functions, and the retryability
  predicate comes from `libfdb_c`, not a hand-written table. Carry-forward item 2: closed.
- Paged-scan completeness: M4 kills `tests/scan.rs` and nothing else, and the fixture grounds its
  own page-boundary assumption against the live server (`scan.rs:163`). Carry-forward item 3:
  closed.
- The `snapshot = false` precondition read (`lib.rs:505`), the observed-miss branch
  (`lib.rs:510`), the `conditional`-only 1020 classification (`lib.rs:123`) and the blind-fault
  surfacing (`lib.rs:680`) are each killed by a live test (M5, M8, M6, M7). No conflict-semantics
  mutation I could construct survived undetected.
- `cargo clippy -p wyrd-metadata-fdb --features fdb --all-targets` is clean, and
  `cargo deny --all-features check licenses` passes with the patch (and fails without the ISC
  line) — the maintainer's ruling is correctly encoded, whatever the comment says.
- `xtask/src/main.rs:411-417` correctly declines to inherit `run_tikv_test`'s 5× retry, closing
  the flake-laundering hazard named in `brief.md:210`.
- Corroborating the carried-forward **brief defect**, with evidence rather than reading:
  `brief.md:124-125` designates `tests/conformance.rs` the "primary — must go red→green" witness
  for the 1020→`Conflict` rule. It is not. Under M6 (drop the guard) *and* under M5 (snapshot
  precondition read, which destroys lost-race detection outright) the conformance leg stays
  **green**. `tests/contention.rs` is the only live witness. Do reported this correctly and did
  not fork the shared suite.
