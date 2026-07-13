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

Review task: add the first FoundationDB-backed `MetadataStore` implementation, gated behind `fdb`, with shared conformance and contention coverage for issue 438.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance target is explicit: a feature-gated FDB backend plus shared conformance/contention proof against a live single-node server, with server selection out of scope (`brief.md:22`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | The live demonstrated-red decision remains owed: this sandbox could not access Docker, so I could not rerun the required FDB red assertions against the real server (`brief.md:31`; `crates/metadata-fdb/tests/contention.rs:46`). |
| C3 Change | PASS | The patch adds the missing FDB workspace member and implementation surface the spec calls for (`Cargo.toml:18`; `crates/metadata-fdb/src/lib.rs:411`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Feature-on FDB tests compile here, but the live red→green leg was not exercised because `docker info` was denied at `/var/run/docker.sock`; `cargo xtask fdb-conformance` therefore skipped (`xtask/src/main.rs:291`; `crates/metadata-fdb/tests/conformance.rs:56`). |
| C5 Causal adequacy | NEEDS-HUMAN | Maintainer must decide whether a not-yet-selected backend is causally sufficient for this slice, because production construction is explicitly deferred and only tests currently exercise the store (`crates/metadata-fdb/src/lib.rs:7`; `brief.md:130`). |
| T1 Structure | PASS | The implementation is isolated as a concrete backend crate, with the throwaway FDB topology under `deploy/` rather than coupled into library crates (`Cargo.toml:18`; `deploy/fdb-single-node/docker-compose.yml:1`). |
| T2 Shape | PASS | The FDB dependency wall is off by default and backend deps are optional, preserving feature-off workspace builds while exposing a dedicated `fdb` feature (`crates/metadata-fdb/Cargo.toml:10`; `crates/metadata-fdb/Cargo.toml:21`). |
| T3 Runtime | NEEDS-HUMAN | The runtime behavior that matters is live FDB conflict classification, and I could only confirm clean feature-off skips plus feature-on compilation, not a real cluster run (`crates/metadata-fdb/tests/contention.rs:38`; `crates/metadata-fdb/src/lib.rs:491`). |
| T4 Contribution | NEEDS-HUMAN | New `foundationdb` dependency adoption is human-only by the brief/INTEGRATION rule; prior-art by affected path was mechanically empty via `git log --all -- crates/metadata-fdb` (`Cargo.toml:105`; `brief.md:165`). |
| T5 Judgment | NEEDS-HUMAN | Human sign-off must accept the dependency/runtime deferral package: `cargo xtask ci` hit an unrelated loopback bind host failure, while the FDB live leg skipped for Docker permission (`crates/chunkstore-grpc/tests/list_delete.rs:55`; `xtask/src/main.rs:295`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Fitness-to-purpose is always a human sign-off item, and here it specifically hinges on rerunning `cargo xtask fdb-conformance` where Docker and the 7.3 FDB client/server are actually available (`brief.md:113`; `xtask/src/main.rs:396`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — The live demonstrated-red decision remains owed: this sandbox could not access Docker, so I could not rerun the required FDB red assertions against the real server (`brief.md:31`; `crates/metadata-fdb/tests/contention.rs:46`).
- [ ] C4 Verification (red→green) — Feature-on FDB tests compile here, but the live red→green leg was not exercised because `docker info` was denied at `/var/run/docker.sock`; `cargo xtask fdb-conformance` therefore skipped (`xtask/src/main.rs:291`; `crates/metadata-fdb/tests/conformance.rs:56`).
- [ ] C5 Causal adequacy — Maintainer must decide whether a not-yet-selected backend is causally sufficient for this slice, because production construction is explicitly deferred and only tests currently exercise the store (`crates/metadata-fdb/src/lib.rs:7`; `brief.md:130`).
- [ ] T3 Runtime — The runtime behavior that matters is live FDB conflict classification, and I could only confirm clean feature-off skips plus feature-on compilation, not a real cluster run (`crates/metadata-fdb/tests/contention.rs:38`; `crates/metadata-fdb/src/lib.rs:491`).
- [ ] T4 Contribution — New `foundationdb` dependency adoption is human-only by the brief/INTEGRATION rule; prior-art by affected path was mechanically empty via `git log --all -- crates/metadata-fdb` (`Cargo.toml:105`; `brief.md:165`).
- [ ] T5 Judgment — Human sign-off must accept the dependency/runtime deferral package: `cargo xtask ci` hit an unrelated loopback bind host failure, while the FDB live leg skipped for Docker permission (`crates/chunkstore-grpc/tests/list_delete.rs:55`; `xtask/src/main.rs:295`).
- [ ] Validation — fitness-to-purpose — Fitness-to-purpose is always a human sign-off item, and here it specifically hinges on rerunning `cargo xtask fdb-conformance` where Docker and the 7.3 FDB client/server are actually available (`brief.md:113`; `xtask/src/main.rs:396`).
- [ ] external dependency: libloading v0.8.9 (ISC), a transitive build-dependency of foundationdb-sys via bindgen -> clang-sys — ISC is not on deny.toml's allowlist (deny.toml:25-38). No PDCA gate covers it: `cargo deny check` as run by `run_ci` resolves default features and therefore sees neither `foundationdb` nor `tikv-client` (verified: `cargo deny list | grep -c foundationdb` -> 0). I could not produce licence-wall clearance for the fdb tree, and per the brief I did not edit deny.toml unilaterally. Maintainer call at sign-off (ADR-0003 three-test audit): either allow "ISC" (already required by `ring` behind the pre-existing tikv-client tree, so `cargo deny --all-features check licenses` already fails on pristine main) or reject the foundationdb build-dep tree.

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
- Iteration delta (if iterating): The fix is sound — not the reason for the iterate. The adversary reviewer ran four mutations against a live foundationdb/foundationdb:7.3.77 cluster and could not break the commit contract; the builder captured four demonstrated reds (RED-A/B/C/S) against a real fdbserver. The design is not in question. Three concrete gaps, all in-scope, no open design decisions: 1. deny.toml — MAINTAINER RULING AT SIGN-OFF: ISC is ALLOWED. Add "ISC" to deny.toml's `allow` list with an ADR-0003 §2 rationale comment, in the style of the existing BSD-3-Clause / Zlib entries (name libloading v0.8.9, reached transitively via foundationdb-sys -> bindgen -> clang-sys). This edit is in scope per brief.md:76-77 scope item (5) ("deny.toml if the graph introduces a license not already allowed"); the graph does, and patch.diff does not touch deny.toml. Do was right to stop-and-declare per brief.md:199-200 rather than edit unilaterally — the call is now made, so encode it. Do NOT re-ask this at the next sign-off. Note the hole is invisible to `cargo deny check` as run_ci invokes it (default features never resolve the foundationdb tree; verified `cargo deny list | grep -c foundationdb` -> 0), and that `cargo deny --all-features check licenses` already fails on pristine main via ring behind tikv-client. Fixing that pre-existing failure is NOT in this slice's scope; only the ISC allowlist entry is. 2. The 1021 no-blind-retry rule — brief.md:83-86's headline constraint — is defended by no test. The test named for it, `commit_unknown_result_is_never_conflict_and_never_retried` (contention.rs:104-107), never calls `commit_blind`: it asserts on the pure `classify_commit_error` and greps a Display substring, duplicating the unit tests at lib.rs:174-208. Changing the blind path's guard at lib.rs:956 from `err.is_retryable_not_committed()` to `err.is_retryable()` (1021 IS retryable) leaves conformance, contention and --lib all green. Add a test that exercises commit_blind's retry arm so that mutation goes RED. 3. scan's paging loop (lib.rs:473-485, scan_once) is dead code under test. Replacing the `next_range` match with `return Ok(out)` — a scan that silently returns only its first page — leaves conformance and contention green: every scan in the shared suite returns <=3 keys in one page, so `more()` is never true. Add a paged-scan completeness test mirroring the TiKV peer's crates/metadata-tikv/tests/scan.rs (the #254 binary named at xtask/src/main.rs:218-221) so first-page truncation goes RED. Already cleared — do not re-litigate: - C5 / T5 (a backend built ahead of its production consumer is causally sufficient for this slice): ACCEPTED. Pre-declared at brief.md:147-156. - T4 (adopt foundationdb 0.10, default-features = false, features = ["fdb-7_3"]): ACCEPTED. - The live-run NEEDS-HUMANs the codex reviewer raised (C2, C4, T3, fitness-to-purpose) are a sandbox artifact, not a finding: `codex exec --sandbox workspace-write` denies /var/run/docker.sock, so that reviewer could not reach the cluster. Docker, libfdb_c 7.3 and fdbcli are all present on this host, and both the builder and the adversary ran the live legs for real. Carry forward, so the rebuild is not misled: - brief.md:124-125 designates tests/conformance.rs the "primary — must go red->green" witness for the 1020->Conflict rule. It CANNOT be: contract_rename_race_yields_conflict (metadata-conformance/src/lib.rs:168-227) is strictly sequential, so the racer's precondition read observes the deleted key and returns Ok(Conflict) from the observed-miss branch (lib.rs:505); classify_commit_error is never invoked from conformance.rs and FDB error 1020 never arises there. Both the builder (build-notes.md §3) and the adversary found this independently. The 1020 rule is pinned by tests/contention.rs:175. This is a BRIEF defect, correctly reported by Do — not a shortfall to fix by reshaping the shared suite. Do NOT fork or weaken crates/metadata-conformance to satisfy the brief's sentence. - check-gates C4-verify was scored "pass" on the evidence string "no pre-patch state to isolate a RED against" — verbatim the ground brief.md:39-40 forbids. The real reds exist (build-notes.md §2). The row's evidence should cite them rather than rationalize absence. - xtask/src/main.rs:417 retries a failing test binary 5x, treating an assertion failure like a cluster-still-settling connection failure. Inherited from the TiKV precedent the brief told Do to mirror, so not invented here — but with (2) and (3) unfixed it can convert a future flake in the sole 1020-pinning test into a green. Worth a look while adding the tests above.
- By / date: Eduard Ralph / 2026-07-09

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_438 — the reviewer leaf's `codex exec --sandbox workspace-write` (pdca.toml:342) denies `/var/run/docker.sock`, so the reviewer cannot run container-backed tests and emits NEEDS-HUMAN on C2/C4/T3/fitness where the host in fact has Docker; consider a grounding/sandbox policy for live-fixture bundles.
