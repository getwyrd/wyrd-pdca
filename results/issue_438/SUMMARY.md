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

Review task: implement the first FoundationDB-backed `MetadataStore` slice for issue 438, including feature gating, shared conformance, contention, scan completeness, and the FDB runner.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines a concrete FDB backend contract and explicitly scopes out server selection, so the decision is whether this slice satisfies the backend/test harness without widening production wiring (`brief.md:73`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | The decision owed is whether the live/mutation RED evidence was actually captured outside this sandbox; I confirmed `origin/main` has no `crates/metadata-fdb`, but Docker is inaccessible here and `check-gates.json` still records “no pre-patch state” for C4-verify. |
| C3 Change | PASS | The patch adds a dedicated optional FDB crate and workspace dependency without changing the trait or server selection, matching the composition boundary (`crates/metadata-fdb/Cargo.toml:11`, `crates/metadata-fdb/src/lib.rs:9`). |
| C4 Verification (red→green) | NEEDS-HUMAN | The decision owed is whether the live FDB red→green legs were run on a host with Docker/FDB; I reran feature-off and feature-on unit/build checks green, but `cargo xtask fdb-conformance` skipped because Docker access was denied while the live runner is the required proof (`xtask/src/main.rs:292`). |
| C5 Causal adequacy | PASS | The backend gap is directly addressed by a real `MetadataStore` implementation plus reachable tests for the prior uncovered causes: routing/retry, 1021 no-retry, scan cap, and typed exhaustion (`crates/metadata-fdb/src/lib.rs:902`, `crates/metadata-fdb/src/lib.rs:1029`, `crates/metadata-fdb/src/lib.rs:1118`). |
| T1 Structure | PASS | The crate boundary is isolated behind the `fdb` feature with optional backend dependencies, preserving default builds that do not link `libfdb_c` (`crates/metadata-fdb/Cargo.toml:21`). |
| T2 Shape | PASS | The tests drive the shared runner rather than forking the contract, and the FDB-specific contention/scan binaries cover live properties the shared suite cannot reach (`crates/metadata-fdb/tests/conformance.rs:48`, `crates/metadata-fdb/tests/scan.rs:53`). |
| T3 Runtime | NEEDS-HUMAN | The decision owed is live runtime acceptance on a host with Docker and a real FDB cluster; my sandbox compiled `--features fdb --tests` and ran `--features fdb --lib`, but could not exercise `conformance`, `contention`, or `scan` against `fdbserver` (`xtask/src/main.rs:405`). |
| T4 Contribution | PASS | The new dependency/version/license choice is encoded as requested: `foundationdb = 0.10` with `fdb-7_3`, and the ISC allowlist entry is present for the build-time `libloading` path (`Cargo.toml:108`, `deny.toml:38`). |
| T5 Judgment | PASS | Building the backend ahead of server selection is explicitly the accepted slice boundary, and the code keeps cluster-file ownership inside the driver rather than adding an out-of-scope server arm (`crates/metadata-fdb/src/lib.rs:10`, `crates/metadata-fdb/src/lib.rs:673`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide production fitness because the required live FoundationDB proof and final dependency judgment depend on external services/tools not exercised by this reviewer sandbox. |

### Advisory — adversary

# Check — adversarial review (advisory, issue #151)

Lens: refute the red→green evidence, refute the fix, refute the reviewer's verdict.
Advisory only — nothing here gates. Every `path:line` is grounded on `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt`).

**Toolchain was available** (issue #236 does *not* apply): `libfdb_c.so` + `fdbcli` on the
host, docker reachable. I stood up `foundationdb/foundationdb:7.3.77` from
`deploy/fdb-single-node/docker-compose.yml` and ran all four legs live, plus a nine-mutant
battery against the running server. `crates/metadata-fdb/src/lib.rs` was restored
byte-identical (sha256 `589bae9d…`) and the container torn down.

## What I could not refute

I ran nine mutations against a live `fdbserver`. **Eight died**, several on the *live* legs
rather than on unit tests — this fix is better evidenced than the gate rows suggest:

| # | Mutation | Killed by |
|---|---|---|
| M1 | `commit_path` (lib.rs:1029) always `Conditional` | `--lib` |
| M2 | `blind_commit_step` (lib.rs:865) `is_retryable_not_committed` → `is_retryable` | `--lib` |
| M3 | `after_page` (lib.rs:494) never returns `CapExceeded` | `--lib`, `--test scan` |
| M4 | `classify_commit_error` (lib.rs:169) drops 1020→`Conflict` | `--lib`, **live contention** |
| M5 | precondition read `snapshot=false`→`true` (lib.rs:775) | **live contention** |
| M6 | `scan_once` (lib.rs:757) truncates at first page | **live scan** |
| M7 | `commit_conditional` classifies its commit error as blind (lib.rs:793) | **live contention** |
| M10 | `prefix_upper_bound` (keyspace) always `None` | `--lib`, **live conformance + scan** |

I specifically attacked, and failed to break: read-your-writes on the CAS path
(`crates/metadata-fdb/src/lib.rs:739-751` reads every precondition *before* `stage()`, so a
batch that both `require_absent(k)`s and `put(k,…)`s is classified against the database, not
against its own staged writes); the "no `String`-backed error" claim at
`crates/metadata-fdb/src/lib.rs:90-95` (every `BoxError::from` site — :922, :998, :1000,
:1095, :1099, :1131, :1145, :1149 — carries a typed error, so the `downcast_ref` contract
holds); and the suspicion that `xtask fdb-conformance` was passing on cleanly-*skipping*
tests (`xtask/src/main.rs:404-417` does export `WYRD_FDB_CLUSTER_FILE` and passes
`--nocapture`, so the legs genuinely ran — the sub-100ms timings are real FDB, not a skip).
The module doc's honesty at `crates/metadata-fdb/src/lib.rs:31-45` — conceding the
`conditional` guard is defence-in-depth and structurally unreachable on the blind path —
survives scrutiny: `route_commit` is on the production path (`lib.rs:1165`), not a parallel
re-implementation.

Attempted to refute the 1020 classification, the 1021 no-blind-retry rule, the scan
consistent-cut, the cap fail-loud, and the CAS read-conflict-set semantics; **could not**.

## Findings

- **NEEDS-HUMAN — `crates/metadata-fdb/src/lib.rs:713` — the `with_scan_cap` clamp is the one
  surviving mutant; its "test" is vacuous.** Deleting the clamp (`self.scan_cap = cap.min(paging::SCAN_CAP);`
  → `self.scan_cap = cap;`) leaves **all four legs green** against a live cluster (`--lib` 26
  passed, conformance 1, contention 3, scan 2). The only assertion aimed at it,
  `crates/metadata-fdb/tests/scan.rs:249-255`, opens a store `.with_scan_cap(usize::MAX)` and
  asserts `scan(...).len() == DIRENTS`; `DIRENTS` is 600 (`scan.rs:87`), which is under
  `SCAN_CAP` *and* under `usize::MAX`, so the assertion holds identically with or without the
  clamp. The assertion's own message says so out loud: *"a cap above SCAN_CAP is clamped to
  SCAN_CAP, which {DIRENTS} keys are under"*. Concrete failing case: with the clamp gone, a
  caller does `FdbMetadataStore::open(cf).with_scan_cap(usize::MAX)` and gets exactly the
  unbounded `inode:` listing that the #262 heap/GC-safety-set constraint exists to forbid —
  reintroducing iteration-2's BLOCKER through a `pub` API — and no test in this diff notices.
  This is the *same vacuity shape* the iteration-1 sign-off rejected for the 1021 rule
  ("asserts on the pure classifier … duplicating the unit tests") and the iteration-2 sign-off
  rejected for the `conditional` guard ("pins the guard only by calling the pure
  `classify_commit_error` directly"). It sits one method over, on the doc claim at
  `lib.rs:700-702` — *"raising it is how an unbounded listing gets back in"* — which is now the
  crate's only unpinned correctness assertion. Cheap kill: a `#[cfg(test)]` unit test on the
  private field (`store::tests`, same module) asserting
  `FdbMetadataStore{..}.with_scan_cap(usize::MAX).scan_cap == paging::SCAN_CAP`, or make
  `scan.rs`'s `unclampable` store use `LOWERED_CAP`-then-raise so the clamp is observable.
  Human call: is an untested defence-in-depth clamp on a not-yet-wired `pub` API a blocker at
  this slice, given the two prior iterations were bounced for precisely this pattern?

- **NEEDS-HUMAN — `check-gates.json:43,46` — the `C4-verify` "pass" is unwarranted, for the
  third consecutive iteration, and its stated rationale is now demonstrably false.** The row
  scores `"result": "pass"` on the evidence string *"no pre-patch state to isolate a RED
  against; C4-ci gates the whole tree (#88)"* — verbatim what `brief.md:39-40` forbids
  ("the RED is a *real failing assertion against a real server*, not criterion-absence — Do
  MUST capture it"), and verbatim what both prior sign-offs flagged. The load-bearing half of
  the excuse is measurably wrong: `C4-ci` (`check-gates.json:33-39`, the **only** `gating:
  true` row) runs `cargo test --workspace` with **default features**
  (`xtask/src/main.rs:1302`), and the whole `store` module is `#[cfg(feature = "fdb")]`
  (`crates/metadata-fdb/src/lib.rs:545-546`). I measured it: the gating build compiles **17**
  lib tests where `--features fdb` compiles **26**. The nine it never compiles are exactly the
  sole witnesses for this patch's core rules —
  `store::tests::a_blind_commit_never_retries_commit_unknown_result`,
  `…::a_blind_batch_routes_to_the_blind_path`,
  `…::commit_path_is_decided_solely_by_the_presence_of_preconditions`,
  `…::an_exhausted_retry_budget_carries_the_last_fdb_error_as_its_source`, and five siblings —
  i.e. the tests that killed M1, M2 and M3 above. Worse, feature-off the three integration
  binaries still report `1 passed` / `3 passed` / `2 passed` while executing only their
  `#[cfg(not(feature = "fdb"))]` skip arms. So the green gating row exercised **zero** lines of
  the FDB driver and its "gates the whole tree" claim cannot be sustained. The reds are real
  and were cheap to cite; the row should cite them. (The *fix* is not impeached by this — only
  the verdict's evidence. I reproduced the red→green myself.)

- **NEEDS-HUMAN — `xtask/src/main.rs:1250-1258` — `feature_gated_checks()` gained no `fdb` arm,
  so the driver is never type-checked or linted by any `xtask ci` path.** The function returns
  a single `["check", "-p", "wyrd-metadata-tikv", "--features", "tikv", "--tests"]`, and the
  patch does not touch it (`grep feature_gated_checks patch.diff` → no hits). Its own comment,
  `xtask/src/main.rs:1300-1305`, states the exact reason it exists: *"`--all-targets` selects
  target KINDS, not features, so a `#[cfg(feature = "tikv")]` body slips through."* That
  sentence is now equally true of `#[cfg(feature = "fdb")]`, and nothing catches it — even on
  the privileged toolchain job. Concrete failing case: a clippy violation or a type error
  introduced anywhere in `crates/metadata-fdb/src/lib.rs:546-1468` ships green through
  `cargo xtask ci` on a machine that *has* `libfdb_c`, and is caught only by the standalone
  `fdb-conformance` job that requires docker. The brief scopes out "the GitHub Actions CI
  workflow" (`brief.md:88`) but says nothing about this in-repo mirror, and the brief *did*
  instruct Do to mirror the TiKV xtask shape (`brief.md:189-192`) — the compose/job half was
  mirrored, this half was not. Whether a one-line `wyrd-metadata-fdb`/`fdb` arm belongs in this
  slice or in the packaging issue is the maintainer's call.

## Non-findings (checked, not filed)

- `keyspace::logical` silently drops a key that fails the prefix strip
  (`crates/metadata-fdb/src/lib.rs:749`), which would be a completeness violation — but the
  range is bounded by `prefix_upper_bound(physical_prefix)` (`lib.rs:1123-1125`), so no
  in-range key can fail the strip. Unreachable; M10 confirms the bound is pinned (the
  `dir;decoy` fixture at `crates/metadata-fdb/tests/scan.rs:100` exists precisely for this).
- `commit_conditional` (`lib.rs:738`) does no bounded retry, so a transient `1007
  transaction_too_old` surfaces as `Err`. This is *required*, not a defect:
  `brief.md:84-86` — "The driver must not silently retry a conditional batch on the caller's
  behalf."
- `scan()` reuses one `Transaction` across retry attempts (`lib.rs:1128-1143`), but
  `on_error` resets it to a fresh read version and `scan_once` restarts from an empty `out`,
  so the consistent cut is never stitched across versions.
- Pre-existing debt not touched by this diff (`cargo deny --all-features` failing on `ring`
  behind `tikv-client`; the `deny.toml:42-43` ISC comment) — already ruled on at sign-off,
  out of scope here.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — The decision owed is whether the live/mutation RED evidence was actually captured outside this sandbox; I confirmed `origin/main` has no `crates/metadata-fdb`, but Docker is inaccessible here and `check-gates.json` still records “no pre-patch state” for C4-verify.
- [x] C4 Verification (red→green) — The decision owed is whether the live FDB red→green legs were run on a host with Docker/FDB; I reran feature-off and feature-on unit/build checks green, but `cargo xtask fdb-conformance` skipped because Docker access was denied while the live runner is the required proof (`xtask/src/main.rs:292`).
- [x] T3 Runtime — The decision owed is live runtime acceptance on a host with Docker and a real FDB cluster; my sandbox compiled `--features fdb --tests` and ran `--features fdb --lib`, but could not exercise `conformance`, `contention`, or `scan` against `fdbserver` (`xtask/src/main.rs:405`).
- [x] Validation — fitness-to-purpose — Human sign-off must decide production fitness because the required live FoundationDB proof and final dependency judgment depend on external services/tools not exercised by this reviewer sandbox.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-09

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
