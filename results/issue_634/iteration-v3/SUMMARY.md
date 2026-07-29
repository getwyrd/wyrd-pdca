# Result — issue 634 / scan-page-seam

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `MetadataStore` gains a **bounded, cursor-keyed range scan** whose semantics are
- Success criterion: two **NEW** test files (see `Test file`), both compiled and run by the
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the `scan_page` trait method and its normative doc contract

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 72 mutants tested in 29s: 19 missed, 27 caught, 26 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add a bounded, cursor-keyed `MetadataStore::scan_page` seam with uniform semantics and native implementations across redb, FoundationDB, TiKV, and both DST stores.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | No specification decision remains open: order, degenerate exclusive cursors, termination/no-skip, cap clamping, zero-bound rejection, and the required-method choice are normative at `crates/traits/src/lib.rs:980` and `crates/traits/src/lib.rs:1041`. |
| C2 Reproduction (red pre-fix) | PASS | An independent clean-base run with only the new targets failed before executing tests because the trait method/helpers were absent, while the patched violating-store matrix assertion-fails each old-compatible wrong behavior described at `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:15`. |
| C3 Change | PASS | The cap escape is complete at the required seam: redb, FoundationDB, TiKV, and both simulations provide range-native pages rather than a production `scan()` fallback (`crates/metadata-redb/src/lib.rs:162`, `crates/metadata-fdb/src/lib.rs:1891`, `crates/metadata-tikv/src/lib.rs:1084`, `crates/dst/tests/support/mod.rs:381`). |
| C4 Verification (red→green) | PASS | Independent reruns produced compile-red on the clean base, then 18/18 and 10/10 green for the added targets; full `cargo xtask ci`, DST, all four feature-clippy rows, and live FoundationDB also passed, so the recorded workspace-test failure did not reproduce (`crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:1`, `crates/metadata-redb/tests/scan_page.rs:92`). |
| C5 Causal adequacy | PASS | The missing primitive is corrected at native range seams without a capability probe or runtime fallback; the reproduced 19 mutation survivors were 18 optional-backend bodies excluded by the scanner's feature set plus one equivalent `>`→`>=` mutation shadowed by the preceding prefix arm at `crates/traits/src/lib.rs:496`. |
| T1 Structure | PASS | The required method and dev-only test-double helper enforce the intended dependency direction, so a production backend cannot inherit the cap-limited shim (`crates/traits/src/lib.rs:1041`, `crates/testkit/src/lib.rs:759`). |
| T2 Shape | PASS | Shared limit/start/cursor decisions and the two centralized suite runners remove per-backend semantic and test-list drift (`crates/traits/src/lib.rs:414`, `crates/traits/src/lib.rs:494`, `crates/traits/src/lib.rs:518`, `crates/metadata-conformance/src/lib.rs:1208`, `crates/metadata-conformance/src/lib.rs:1243`). |
| T3 Runtime | NEEDS-HUMAN | A maintainer must provide a TiKV topology compatible with the client's API mode and rerun `cargo xtask tikv-conformance` until both runners pass—the live run failed with `InvalidKeyMode { storage_api_version: V2 }` during the first commit, before `scan_page`, leaving TiKV parity unexercised at `crates/metadata-tikv/tests/conformance.rs:53` and `crates/metadata-tikv/tests/conformance.rs:69`. |
| T4 Contribution | NEEDS-HUMAN | A maintainer must inspect or rerun the two unavailable batched-review findings and verify closed/rejected work by affected path—merged affected-path history and `git log -S scan_page --all` found proposal history only, but the finding artifact and rejected-work corpus were unavailable, so contribution readiness is not mechanically settled. |
| T5 Judgment | PASS | No independently reproducible or source-grounded patch defect remains; acceptance now turns only on the explicit TiKV, contribution-history, and fitness decisions recorded in this table. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | A maintainer must decide whether this presently unconsumed seam's semantics and scale envelope are fit for the future #636/#637 retirement and GC walks—mechanical conformance does not validate the operational 1.78 M-key use case described at `docs/design/architecture/05-building-block-view.md:204`. |

### Advisory — adversary

# Adversarial review — issue 634 / scan-page-seam (advisory, non-gating)

Attacked: the red→green evidence, the four normative clauses, the five `scan_page`
bodies, and the two red gates. Re-ran everything reproducible at `$PDCA_TARGET`
(`cargo test --workspace --exclude wyrd-dst` ×2, the `--cfg madsim` DST leg, the two
feature-gated clippy rows) and fuzzed the redb backend against an independent oracle.
Three findings; the rest of the attack failed and is recorded as such.

- **NEEDS-HUMAN [impl]** — `crates/traits/src/lib.rs:518` (`page_cursor`) with
  `crates/traits/src/lib.rs:1009` (contract clause 3): **the shipped contract makes
  "short page" mean "prefix exhausted", but nothing in it obliges a backend to fill a
  page** — so a backend whose substrate legitimately under-fills one answers
  `next: None` with keys still behind it, which is exactly the silent skip this slice
  exists to prevent. Clause 3 as written ("`next` is `Some(last key returned)` while
  more may remain, `None` **only** when the prefix is exhausted") is satisfied by a
  store that returns 3 of 6 pairs with `next = Some(last)`; `page_cursor` instead
  infers exhaustion from `items.len() < limit`. The suite silently enforces the
  missing rule — and its own comment denies enforcing it. Concrete case, run against
  the shipped clauses: a `LazyPagedStore` honouring clauses 1–4 and
  `items.len() <= min(limit, cap)` but capping each page at 3 pairs goes **red at three
  places** — `crates/metadata-conformance/src/lib.rs:464` ("one page of the whole
  prefix must come back in raw byte order", left 3 keys / right 6),
  `:563` ("a cursor below the range skips nothing", left 3 / right 4) and `:917`
  ("a limit above the store's cap must be clamped… never refused", left 3 / right 5) —
  while `crates/metadata-conformance/src/lib.rs:305-310` states the opposite in prose
  ("the contract lets a store answer a short non-final page"). Two of those three are
  also mis-messaged: `:917` blames the cap clamp for a maximality failure, and `:563`
  is the count-based assertion shape the repo rubric calls out. Fix is cheap and
  belongs in Do: state maximality (or "a short page means exhausted") as a numbered
  clause on `MetadataStore::scan_page`, correct the `LAP_BUDGET` comment, and reword
  `:917`. It is load-bearing rather than cosmetic — the two backends whose behavioural
  evidence is deferred (fdb, tikv) are the ones whose substrates can hand back a
  partial range read, and `crates/metadata-tikv/src/lib.rs:1141` derives `next` from
  `page_cursor` on exactly that assumption.

- **NEEDS-HUMAN [impl]** — `crates/traits/src/lib.rs:1054` claims of
  `wyrd_testkit::test_double_scan_page` that testkit "is a dev-dependency everywhere,
  never a dependency", and `crates/testkit/src/lib.rs:772-780` upgrades that to "a
  production `MetadataStore` body naming this function does not compile… what would
  otherwise be a convention is a build error". **Both are false as stated**:
  `crates/coordination-mem/Cargo.toml:16` lists `wyrd-testkit` under
  `[dependencies]` (with the comment "production code is written against testkit's
  abstractions"), as does `crates/metadata-fault-conformance/Cargo.toml:20`, and
  `cargo tree -p wyrd-server -e normal -i wyrd-testkit` resolves
  `wyrd-testkit → wyrd-coordination-mem → wyrd-server`. The new `pub async fn
  test_double_scan_page` — documented as a cap-inheriting `scan()` shim, i.e. exactly
  the body #508's 4th attempt was rejected for — is therefore compiled into the
  production server binary, and the "build error, not a convention" guarantee holds
  only for the three metadata crates by accident of their *current* dev-dep edges.
  This is the same visibility risk iteration 1 was asked to reduce, moved rather than
  removed; a `#[cfg(feature = "test-doubles")]` gate (or an accurate doc) would settle
  it. (Same doc block, minor: `crates/traits/src/lib.rs:1038-1039` states "a page
  **never** fails with `ScanCapExceeded`" as a trait-wide contract, while ~34
  in-workspace impls raise exactly that via the helper by design.)

- **NEEDS-HUMAN** — `check-gates.json` reports the sole gating build row red
  ("xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101"),
  and **I cannot reproduce it**: at `$PDCA_TARGET` with the patch applied that exact
  command exited 0 twice (153 test binaries green, incl. `metadata-redb/tests/scan_page.rs`
  10/10 and `metadata-conformance/tests/scan_page_demonstrated_red.rs` 18/18), and the
  DST leg the sim stores depend on — `RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=50 cargo
  test -p wyrd-dst`, `xtask/src/main.rs:1602-1608` — also exited 0. So the red is
  either a flake or an environment artefact of the gate run, but an unexplained red on
  the one gating row cannot be waived by my green: a human should read the gate log and
  name the failing binary (if it is a flaky test elsewhere in the workspace, that is a
  repo item, not this patch's). Verdict on that row provisional (issue #236).

- (Not a refutation, recorded because the human sees the red) **C5's 19 missed mutants
  are fully explained**: 18 sit in `crates/metadata-fdb/src/lib.rs` and
  `crates/metadata-tikv/src/lib.rs`, the two bodies the brief declares off-Check, and
  the 19th — `crates/traits/src/lib.rs:499:32 replace > with >=` in `page_start` — is an
  **equivalent mutant**: `cursor == prefix` is absorbed by the `starts_with` arm at
  `:497`, so `>` and `>=` are observationally identical. Every other seam mutant
  (`page_limit`, both `page_start` guards, `page_cursor`'s `>=`, redb's `!` and `>=`,
  the testkit helper's `>`) is caught.

## Attempted and could not refute

- **The redb body, fuzzed against an independent oracle** (400 seeds × 40 random probes
  + 24 full walks each; prefixes `""`, `p`, `p:`, `o:`, `q`, `p\xff`; keys containing
  `0x7f/0x80/0xff/é` and prefix-of-prefix pairs; cursors below / at / inside / past the
  range and non-existent; limits 1–7 and `usize::MAX`; caps 1, 2, 3, 5, 8, 2^20): **zero
  divergences** from "keys under prefix strictly greater than `after`, truncated to
  `min(limit, cap)`", every walk complete, in order and terminating, every zero-bound
  refused with `ZeroPageLimit`. The iteration-1 unbounded-page defect at cap 0 is
  genuinely fixed (`crates/metadata-redb/src/lib.rs:170`, `crates/traits/src/lib.rs:414`).
- **The `page_start` upper-bound equivalence** (`crates/traits/src/lib.rs:494-503`) — I
  could not find a `(prefix, cursor)` pair where `cursor > prefix && !starts_with` differs
  from `cursor >= upper_bound(prefix)`, including all-`0xff` and empty prefixes.
- **Leg D's non-vacuity** — the seven violating doubles are real: each `#[should_panic]`
  string matches the clause's own message, and each double passes the pre-existing
  sequential clauses, so the reds are not compile-shaped.
- **The deferred backends compile**: `cargo clippy -p wyrd-metadata-fdb --features fdb
  --tests` and `-p wyrd-metadata-tikv --features tikv --tests` are both clean here.
- **The `tikv-client` panic citation** repeated in three doc blocks is accurate —
  `tikv-client-0.4.0/src/transaction/buffer.rs:129` is `self.entry_map.range(...)` on a
  `BTreeMap`, which panics when start > end; the `PastPrefix` arm is a real fix, not
  decoration.
- **Docs currency**: `06-runtime-view.md` / `08-crosscutting-concepts.md` contain no
  paragraph claiming the store offers only a whole-namespace scan, so
  `05-building-block-view.md:204` is the correct and sufficient update.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T3 Runtime — A maintainer must provide a TiKV topology compatible with the client's API mode and rerun `cargo xtask tikv-conformance` until both runners pass—the live run failed with `InvalidKeyMode { storage_api_version: V2 }` during the first commit, before `scan_page`, leaving TiKV parity unexercised at `crates/metadata-tikv/tests/conformance.rs:53` and `crates/metadata-tikv/tests/conformance.rs:69`. — cleared at sign-off: TiKV takes a backseat here; accepted as long as redb and FoundationDB are green (both are).
- [x] T4 Contribution — A maintainer must inspect or rerun the two unavailable batched-review findings and verify closed/rejected work by affected path—merged affected-path history and `git log -S scan_page --all` found proposal history only, but the finding artifact and rejected-work corpus were unavailable, so contribution readiness is not mechanically settled. — cleared at sign-off: prior-art search found only the proposal document, no prior implementation; accepted as sufficient.
- [x] Validation — fitness-to-purpose — A maintainer must decide whether this presently unconsumed seam's semantics and scale envelope are fit for the future #636/#637 retirement and GC walks—mechanical conformance does not validate the operational 1.78 M-key use case described at `docs/design/architecture/05-building-block-view.md:204`. — cleared at sign-off: accepted as fit for purpose for the planned #636/#637 consumers.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101 — cleared at sign-off: reran the exact gating command twice in the worktree, both clean (exit 0, 0 failures); agrees with the adversary's two independent reruns. Flake/environment artifact of the original gate run, not a patch defect.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- [x] external dependency: TiKV cluster (api-version V1 / txn mode, host ports 2379+20160 free) — blocks `cargo xtask tikv-conformance`, so the new clause (b) case (v) and the whole shared suite are unverified against the real `TikvMetadataStore`; the tikv-client panic mechanism behind review finding 2 rests on a code read, not a run. — cleared at sign-off: TiKV takes a backseat here; accepted as long as redb and FoundationDB are green (both are).

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected solely on the two live T4 batched-review findings in review-batch.md (both unaddressed, both in the new scan_page conformance test code added by this patch, not the production backends): 1. crates/metadata-conformance/src/lib.rs:465 — the new scan_page clauses assert on returned keys/cursors but never assert on the returned Bytes values, so a backend returning correct keys with stale/swapped/corrupted values would still pass. Add a value assertion to the shared clause(s). 2. crates/metadata-conformance/src/lib.rs:772 — the fixture hard-assumes the first page always fills its limit (2 items), but the brief's own clause (c) explicitly permits a short non-final page (next: Some(...)) with fewer items. Fix the fixture so it does not fail a conforming store that legitimately returns a short page. All other §6 NEEDS-HUMAN items (T3 Runtime/TiKV, T4 Contribution, Validation fitness-to-purpose, C4 gate flake, external TiKV dependency) were cleared at this sign-off — do not re-litigate those; TiKV is accepted as backseat as long as redb/FDB stay green, and the C4 exit-101 gate was independently reproduced clean (flake). Only the two findings above are the reason for iteration.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- TiKV live-cluster conformance (`cargo xtask tikv-conformance`) has been env-blocked across multiple sign-offs on this issue (port conflict with a foreign cluster, `InvalidKeyMode` V2) and keeps recurring as a NEEDS-HUMAN item; investigate a durable fix (dedicated port/topology for this repo's TiKV job) so it stops surfacing at every sign-off.
