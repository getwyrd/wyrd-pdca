# Result — issue 438 / metadata-fdb-store

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: ADR-0042 chose FoundationDB as the production metadata backend, but no
- Success criterion: With the `fdb` feature on and a live single-node `fdbserver`,
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Add a new workspace crate `crates/metadata-fdb` implementing the unchanged

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88).
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add the FoundationDB-backed `MetadataStore` crate and its conformance/contention/scan harness for issue 438, including the carry-forward ISC, 1021 retry, and paged-scan coverage fixes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines a bounded FDB backend slice with feature-off default, live `run_all` conformance, contention semantics, and explicit out-of-scope server selection; the patch maps to that surface in the new workspace member at `Cargo.toml:18` and dependency pin at `Cargo.toml:108`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | The live RED witnesses cannot be re-run here because Docker socket access is denied and no cluster file is available; human must confirm the asserted real-server reds for 1020 classification and paged-scan truncation rather than the non-red `check-gates.json` rationale. |
| C3 Change | PASS | The patch adds the concrete `FdbMetadataStore` behind the unchanged trait, with single-process FDB network boot, prefixed isolation, conditional-vs-blind commit paths, and scan paging at `crates/metadata-fdb/src/lib.rs:397`, `crates/metadata-fdb/src/lib.rs:449`, `crates/metadata-fdb/src/lib.rs:498`, `crates/metadata-fdb/src/lib.rs:536`, and `crates/metadata-fdb/src/lib.rs:467`. |
| C4 Verification (red→green) | NEEDS-HUMAN | I verified `cargo test -p wyrd-metadata-fdb --features fdb --lib` green, including the 1021 retry tests at `crates/metadata-fdb/src/lib.rs:817`, but `cargo xtask ci` failed on sandbox loopback bind permission and `cargo xtask fdb-conformance` skipped because Docker is unavailable here; the live red→green must be confirmed on a host that can run `xtask/src/main.rs:403`. |
| C5 Causal adequacy | PASS | The causal question is the metadata-store contract, not production selection; the driver sends conditional lost races to `Conflict` and blind/unknown outcomes to surfaced errors at `crates/metadata-fdb/src/lib.rs:674`, while the ahead-of-consumer slice is explicitly exercised only by tests and the `xtask` leg. |
| T1 Structure | PASS | The new backend is isolated as its own workspace crate and deploy harness, preserving ADR-0010 composition boundaries: `Cargo.toml:18`, `crates/metadata-fdb/Cargo.toml:21`, and `deploy/fdb-single-node/docker-compose.yml:2`. |
| T2 Shape | PASS | The test shape mirrors the existing backend pattern: default-off clean skips, shared `run_all` with per-tag prefixes, contention tests, and a separate paged-scan witness at `crates/metadata-fdb/tests/conformance.rs:26`, `crates/metadata-fdb/tests/conformance.rs:56`, `crates/metadata-fdb/tests/contention.rs:55`, and `crates/metadata-fdb/tests/scan.rs:36`. |
| T3 Runtime | NEEDS-HUMAN | The runtime behavior that matters is live FDB, not skip-arm success; this sandbox only exercised skip arms for `crates/metadata-fdb/tests/conformance.rs:28`, `crates/metadata-fdb/tests/contention.rs:57`, and `crates/metadata-fdb/tests/scan.rs:37`, so a human must run `cargo xtask fdb-conformance` with Docker and `libfdb_c`. |
| T4 Contribution | NEEDS-HUMAN | `foundationdb` is a new dependency and INTEGRATION marks dependency adoption human-only; local history for affected paths was empty, but GitHub closed/rejected PR search could not be checked due network failure, and the maintainer must clear the ADR-0003 dependency audit for `Cargo.toml:108` and ISC allowlist entry at `deny.toml:45`. |
| T5 Judgment | PASS | The patch avoids broadening production selection and keeps the FDB proof in an explicit xtask job, with all four FDB legs run exactly once rather than retrying assertion failures at `xtask/src/main.rs:403` and `xtask/src/main.rs:423`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether the live single-node FDB evidence satisfies issue 438’s production-backend readiness threshold, because this reviewer could not exercise the required Docker-backed real-server run despite the harness at `xtask/src/main.rs:292`. |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — The live RED witnesses cannot be re-run here because Docker socket access is denied and no cluster file is available; human must confirm the asserted real-server reds for 1020 classification and paged-scan truncation rather than the non-red `check-gates.json` rationale.
- [x] C4 Verification (red→green) — I verified `cargo test -p wyrd-metadata-fdb --features fdb --lib` green, including the 1021 retry tests at `crates/metadata-fdb/src/lib.rs:817`, but `cargo xtask ci` failed on sandbox loopback bind permission and `cargo xtask fdb-conformance` skipped because Docker is unavailable here; the live red→green must be confirmed on a host that can run `xtask/src/main.rs:403`.
- [x] T3 Runtime — The runtime behavior that matters is live FDB, not skip-arm success; this sandbox only exercised skip arms for `crates/metadata-fdb/tests/conformance.rs:28`, `crates/metadata-fdb/tests/contention.rs:57`, and `crates/metadata-fdb/tests/scan.rs:37`, so a human must run `cargo xtask fdb-conformance` with Docker and `libfdb_c`.
- [x] T4 Contribution — `foundationdb` is a new dependency and INTEGRATION marks dependency adoption human-only; local history for affected paths was empty, but GitHub closed/rejected PR search could not be checked due network failure, and the maintainer must clear the ADR-0003 dependency audit for `Cargo.toml:108` and ISC allowlist entry at `deny.toml:45`.
- [x] Validation — fitness-to-purpose — Human sign-off must decide whether the live single-node FDB evidence satisfies issue 438’s production-backend readiness threshold, because this reviewer could not exercise the required Docker-backed real-server run despite the harness at `xtask/src/main.rs:292`.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Sign-off rationale (issue 438 / metadata-fdb, iteration 2 -> 3): The design is sound and is NOT the reason for the iterate. All three carry-forward gaps from iteration 1 are genuinely CLOSED, verified by the adversary against a live foundationdb/foundationdb:7.3.77 cluster: (1) the ISC allowlist entry is encoded and load-bearing; (2) the 1021 no-blind-retry rule is now bound by the production blind_commit_loop/blind_commit_step (mutation is_retryable_not_committed() -> is_retryable() goes RED on --lib); (3) tests/scan.rs kills first-page truncation and grounds its own page-boundary fixture against the live server. Gap 4 (xtask's 5x retry) is also fixed. Eight mutations against the conflict semantics; none survived undetected. Do NOT re-litigate the design, the seam, or the test shapes. Three concrete findings, all in-scope, no open design decisions: 1. `scan` has no SCAN_CAP. THE BLOCKER. crates/metadata-fdb/src/lib.rs:478-492 (`scan_once`) accumulates `out` with no ceiling. The sibling backend crates/metadata-tikv/src/lib.rs:136-145 documents this cap as "a **correctness constraint, not a tuning knob** (#262)" because "a silently truncated `inode:` scan corrupts GC's never-reclaim safety set (data loss)", and fails loud with `ScanCapExceeded` returning NO partial Vec. FDB instead grows the gateway heap unbounded or, past FDB's 5 s transaction limit, takes 1007 at lib.rs:726, restarts the WHOLE scan 5x, and returns the opaque "metadata scan exhausted 5 attempts" string. Mirror the TiKV peer: same 2^20 constant, same `ScanCapExceeded` error type, no partial results, plus a test. No design decision to make — the peer already made it. This is NOT the transaction-envelope item brief.md:98-102 deferred to #437 (that carve-out reasons only about `commit`); the #262 heap/GC bound is a separate constraint, undeclared in the brief, and this diff is the first thing to introduce an FDB `scan`. The shared run_all suite cannot see it, so no gate can. 2. The retry-exhaustion paths break the downcast contract this same diff promises. lib.rs:892-902 states a caller "must be able to `downcast_ref::<foundationdb::FdbError>()`" to tell a transient `transaction_too_old` from a permanent `value_too_large`. But lib.rs:617-619, lib.rs:701-703 and lib.rs:730-732 all exit as `BoxError::from(format!("... exhausted 5 attempts ..."))` — a String-backed error; downcast_ref returns None, destroying exactly the distinction the doc promises. Concrete case: a blind commit failing 5x with 1007 (retryable_not_committed = true, so every attempt takes the Retry arm). Same for `get` and `scan` under five 1007s. `CommitUnknownResult` (lib.rs:137) IS downcastable, so the crate contradicts itself, not merely a peer. Fix: a typed exhaustion error carrying the last FdbError as `source`. 3. The module doc overclaims what pins the invariant's third clause, and two mutants survive. lib.rs:31 says the `conditional` guard "is the whole of the invariant's third clause" and lib.rs:36 says it "is pinned by crates/metadata-fdb/tests/contention.rs". Neither holds on the production blind path: FdbError::from_code(1020) .is_retryable_not_committed() is TRUE, so blind_commit_step (lib.rs:579) always routes 1020 into BlindStep::Retry — `outcome_from_commit_error(err, false)` at lib.rs:613 can never be reached with 1020, and `classify_commit_error(1020, false)` is unreachable from production. Proof: flipping that literal `false` to `true` leaves all four legs green; forcing `let conditional = true` at lib.rs:746 is ALSO green on all four legs. What actually keeps a blind batch out of Conflict is the retry gate, not the guard. contention.rs:286-297 "pins" the guard only by calling the pure classify_commit_error directly — VERBATIM the vacuous shape the iteration-1 sign-off rejected for the 1021 rule (brief.md:210 item 2). The previous review named an instance; the identical pattern sits one function over. Required: (a) correct the doc claim at lib.rs:31,36 — say the guard is defence-in-depth and name what actually pins the clause; (b) add a test that makes the `let conditional = !batch.preconditions.is_empty()` rule at lib.rs:746 load-bearing. Concrete cost of that mutant surviving: it silently removes the bounded retry of 1007 transaction_too_old / 1009 future_version from every blind commit, and no test notices. The guard itself is harmless — keep it; it is the DOC CLAIM and the coverage that are refuted, not the code. Already cleared — do NOT re-ask: - C2 / C4 / T3 / Validation (fitness-to-purpose): CLEARED on the adversary's live-run evidence (Docker + libfdb_c 7.3 + fdbcli present; all four legs run for real, three cold container runs). The codex reviewer's NEEDS-HUMANs on these are a sandbox artifact (`codex exec --sandbox workspace-write` denies /var/run/docker.sock), not a finding. This is the SECOND cycle they have recurred verbatim. - T4 Contribution: CLEARED. foundationdb 0.10 (default-features = false, features = ["fdb-7_3"]) was accepted at iteration 1. The deny.toml ISC entry stands as written. - deny.toml:42-43 — the adversary is factually right that "No ISC-licensed code is linked into a shipped Wyrd binary" is false for `wyrd-server --features tikv` (ring, Apache-2.0 AND ISC, and untrusted, pure ISC, are NORMAL deps via tikv-client -> tonic -> tokio-rustls -> rustls; `grep -ci isc NOTICE` -> 0). MAINTAINER RULING: not a blocker, do not spend a cycle on it. The entry is load-bearing and the ruling is correctly encoded. Leave the comment alone. - C5 / T5 (a backend built ahead of its production consumer is causally sufficient for this slice): ACCEPTED at iteration 1, pre-declared at brief.md:147-156. Carry forward, so the rebuild is not misled: - brief.md:124-125 designates tests/conformance.rs the "primary — must go red->green" witness for the 1020->Conflict rule. It CANNOT be, and this is now confirmed by measurement twice over: under both the drop-the-guard and the snapshot-precondition-read mutations the conformance leg stays GREEN. tests/contention.rs is the only live witness. This is a BRIEF defect, correctly reported by Do. Do NOT fork or weaken crates/metadata-conformance to satisfy the brief's sentence. - check-gates C4-verify is scored "pass" on the evidence string "no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88)" — verbatim what brief.md:39-40 forbids, and flagged at the iteration-1 sign-off. It recurs unchanged. Worse, the claim that C4-ci covers it is false in a way that matters: run_ci builds with DEFAULT features and the entire `store` module is #[cfg(feature = "fdb")] (lib.rs:361), so the green C4-ci gate never compiled the driver, never linted it, and ran none of the four legs — including the four store::tests that are the sole witness for the 1021 rule. Real reds exist and are cheap to cite (build-notes.md §5). Cite them. - contention.rs:73 (blind_write_race_never_reports_conflict) is close to a tautology: a write-only FDB transaction has an empty read-conflict set, so no error is produced, the Ok(CommitOutcome::Conflict) panic arm at contention.rs:225 is unreachable, and any error that did occur is swallowed at contention.rs:230 (`Err(_) => {}`). Its only surviving assertion is `committed >= 1`. Do reported this honestly (build-notes.md §5). Keep it, but do not let it imply more than it proves — and consider reading the key back. - Low severity, fix while in there if cheap: lib.rs:608-616, :697, :726 call `on_error` (a real backoff sleep) after the FINAL failed attempt, charging the caller one full backoff it can never spend; `break` before the last reset. - conformance.rs:58 builds its isolation prefix as wyrd-fdb-conformance/{pid}/{tag}/ with no timestamp, while contention.rs:334 and scan.rs:197 (same diff) both stamp .../{tag}/{nanos}/. Internally inconsistent; masked today only by `compose down -v`.
- By / date: Eduard Ralph / 2026-07-09

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
